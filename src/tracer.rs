use std::{collections::HashMap, ffi::CString, path::PathBuf, process::exit};

use nix::{
    errno::Errno,
    libc::{pid_t, raise, user_regs_struct, SYS_clone, SYS_clone3, AT_EMPTY_PATH, SIGSTOP},
    sys::{
        ptrace::{self, traceme, AddressType},
        signal::Signal,
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{execvp, getpid, ForkResult, Pid},
};

use crate::{
    arch::{is_execveat_execve_quirk, syscall_arg, syscall_no_from_regs, syscall_res_from_regs},
    cli::{Color, TracingArgs},
    inspect::{read_pathbuf, read_string, read_string_array},
    printer::print_execve_trace,
    proc::{read_comm, read_cwd, read_fd, read_interpreter_recursive},
    state::{ExecData, ProcessState, ProcessStateStore, ProcessStatus},
};

pub struct Tracer {
    pub store: ProcessStateStore,
    args: TracingArgs,
    pub color: Color,
    env: HashMap<String, String>,
    cwd: std::path::PathBuf,
}

fn ptrace_syscall_with_signal(pid: Pid, sig: Signal) -> Result<(), Errno> {
    match ptrace::syscall(pid, Some(sig)) {
        Err(Errno::ESRCH) => {
            log::info!("ptrace syscall failed: {pid}, ESRCH, child probably gone!");
            Ok(())
        }
        other => other,
    }
}

fn ptrace_syscall(pid: Pid) -> Result<(), Errno> {
    match ptrace::syscall(pid, None) {
        Err(Errno::ESRCH) => {
            log::info!("ptrace syscall failed: {pid}, ESRCH, child probably gone!");
            Ok(())
        }
        other => other,
    }
}

impl Tracer {
    pub fn new(args: TracingArgs, color: Color) -> color_eyre::Result<Self> {
        Ok(Self {
            store: ProcessStateStore::new(),
            env: std::env::vars().collect(),
            color,
            args,
            cwd: std::env::current_dir()?,
        })
    }

    pub fn start_root_process(&mut self, args: Vec<String>) -> color_eyre::Result<()> {
        log::trace!("start_root_process: {:?}", args);
        if let ForkResult::Parent { child: root_child } = unsafe { nix::unistd::fork()? } {
            waitpid(root_child, Some(WaitPidFlag::WSTOPPED))?; // wait for child to stop
            log::trace!("child stopped");
            let mut root_child_state = ProcessState::new(root_child, 0)?;
            root_child_state.ppid = Some(getpid());
            self.store.insert(root_child_state);
            // restart child
            log::trace!("resuming child");
            ptrace::setoptions(root_child, {
                use nix::sys::ptrace::Options;
                Options::PTRACE_O_TRACEEXEC
                    | Options::PTRACE_O_TRACEEXIT
                    | Options::PTRACE_O_EXITKILL
                    | Options::PTRACE_O_TRACESYSGOOD
                    | Options::PTRACE_O_TRACEFORK
                    | Options::PTRACE_O_TRACECLONE
                    | Options::PTRACE_O_TRACEVFORK
            })?;
            ptrace_syscall(root_child)?; // restart child
            loop {
                let status = waitpid(None, Some(WaitPidFlag::__WALL))?;
                // log::trace!("waitpid: {:?}", status);
                match status {
                    WaitStatus::Stopped(pid, sig) => {
                        log::trace!("stopped: {pid}, sig {:?}", sig);
                        match sig {
                            Signal::SIGSTOP => {
                                log::trace!("sigstop event, child: {pid}");
                                if let Some(state) = self.store.get_current_mut(pid) {
                                    if state.status == ProcessStatus::PtraceForkEventReceived {
                                        log::trace!("sigstop event received after ptrace fork event, pid: {pid}");
                                        ptrace_syscall(pid)?;
                                        state.status = ProcessStatus::Running;
                                    } else if pid != root_child {
                                        log::error!("Unexpected SIGSTOP: {state:?}")
                                    }
                                } else {
                                    log::trace!("sigstop event received before ptrace fork event, pid: {pid}");
                                    let mut state = ProcessState::new(pid, 0)?;
                                    state.status = ProcessStatus::SigstopReceived;
                                    self.store.insert(state);
                                }
                                // https://stackoverflow.com/questions/29997244/occasionally-missing-ptrace-event-vfork-when-running-ptrace
                                // DO NOT send PTRACE_SYSCALL until we receive the PTRACE_EVENT_FORK, etc.
                            }
                            Signal::SIGCHLD => {
                                // From lurk:
                                //
                                // The SIGCHLD signal is sent to a process when a child process terminates, interrupted, or resumes after being interrupted
                                // This means, that if our tracee forked and said fork exits before the parent, the parent will get stopped.
                                // Therefor issue a PTRACE_SYSCALL request to the parent to continue execution.
                                // This is also important if we trace without the following forks option.
                                ptrace_syscall(pid)?;
                            }
                            _ => {
                                // Just deliver the signal to tracee
                                ptrace_syscall_with_signal(pid, sig)?;
                            }
                        }
                    }
                    WaitStatus::Exited(pid, code) => {
                        log::trace!("exited: pid {}, code {:?}", pid, code);
                        self.store.get_current_mut(pid).unwrap().status =
                            ProcessStatus::Exited(code);
                        if pid == root_child {
                            break;
                        }
                    }
                    WaitStatus::PtraceEvent(pid, sig, evt) => {
                        log::trace!("ptrace event: {:?} {:?}", sig, evt);
                        match evt {
                            nix::libc::PTRACE_EVENT_FORK
                            | nix::libc::PTRACE_EVENT_VFORK
                            | nix::libc::PTRACE_EVENT_CLONE => {
                                let new_child = Pid::from_raw(ptrace::getevent(pid)? as pid_t);
                                log::trace!(
                                    "ptrace fork event, evt {evt}, pid: {pid}, child: {new_child}"
                                );
                                if let Some(state) = self.store.get_current_mut(new_child) {
                                    if state.status == ProcessStatus::SigstopReceived {
                                        log::trace!("ptrace fork event received after sigstop, pid: {pid}, child: {new_child}");
                                        state.status = ProcessStatus::Running;
                                        state.ppid = Some(pid);
                                        ptrace_syscall(new_child)?;
                                    } else if new_child != root_child {
                                        log::error!("Unexpected fork event: {state:?}")
                                    }
                                } else {
                                    log::trace!("ptrace fork event received before sigstop, pid: {pid}, child: {new_child}");
                                    let mut state = ProcessState::new(new_child, 0)?;
                                    state.status = ProcessStatus::PtraceForkEventReceived;
                                    state.ppid = Some(pid);
                                    self.store.insert(state);
                                }
                                // Resume parent
                                ptrace_syscall(pid)?;
                            }
                            nix::libc::PTRACE_EVENT_EXEC => {
                                log::trace!("exec event");
                                ptrace_syscall(pid)?;
                            }
                            nix::libc::PTRACE_EVENT_EXIT => {
                                log::trace!("exit event");
                                ptrace_syscall(pid)?;
                            }
                            _ => {
                                log::trace!("other event");
                                ptrace_syscall(pid)?;
                            }
                        }
                    }
                    WaitStatus::Signaled(pid, sig, _) => {
                        log::debug!("signaled: {pid}, {:?}", sig);
                        // TODO: correctly handle death under ptrace
                        if pid == root_child {
                            break;
                        }
                    }
                    WaitStatus::PtraceSyscall(pid) => {
                        let mut regs = std::mem::MaybeUninit::<user_regs_struct>::uninit();
                        let iovec = nix::libc::iovec {
                            iov_base: regs.as_mut_ptr() as AddressType,
                            iov_len: std::mem::size_of::<user_regs_struct>(),
                        };
                        let ptrace_result = unsafe {
                            nix::libc::ptrace(
                                nix::libc::PTRACE_GETREGSET,
                                pid.as_raw(),
                                nix::libc::NT_PRSTATUS,
                                &iovec as *const _ as *const nix::libc::c_void,
                            )
                        };
                        let regs = if -1 == ptrace_result {
                            let errno = nix::errno::Errno::last();
                            if errno == Errno::ESRCH {
                                log::info!(
                                    "ptrace getregs failed: {pid}, ESRCH, child probably gone!"
                                );
                                continue;
                            }
                            return Err(errno.into());
                        } else {
                            unsafe { regs.assume_init() }
                        };
                        let syscallno = syscall_no_from_regs!(regs);
                        // let syscall_info = ptrace::get_syscall_info(pid)?;
                        let p = self.store.get_current_mut(pid).unwrap();
                        // log::trace!("syscall: {syscallno}");
                        if syscallno == nix::libc::SYS_execveat && p.preexecveat {
                            log::trace!("pre execveat {syscallno}");
                            // int execveat(int dirfd, const char *pathname,
                            //              char *const _Nullable argv[],
                            //              char *const _Nullable envp[],
                            //              int flags);
                            let dirfd = syscall_arg!(regs, 0) as i32;
                            let pathname = read_string(pid, syscall_arg!(regs, 1) as AddressType)?;
                            let pathname_is_empty = pathname.is_empty();
                            let pathname = PathBuf::from(pathname);
                            let argv =
                                read_string_array(pid, syscall_arg!(regs, 2) as AddressType)?;
                            let envp =
                                read_string_array(pid, syscall_arg!(regs, 3) as AddressType)?;
                            let flags = syscall_arg!(regs, 4) as i32;
                            let filename = match (
                                pathname.is_absolute(),
                                pathname_is_empty && ((flags & AT_EMPTY_PATH) != 0),
                            ) {
                                (true, _) => {
                                    // If pathname is absolute, then dirfd is ignored.
                                    pathname
                                }
                                (false, true) => {
                                    // If  pathname  is an empty string and the AT_EMPTY_PATH flag is specified, then the file descriptor dirfd
                                    // specifies the file to be executed
                                    read_fd(pid, dirfd)?
                                }
                                (false, false) => {
                                    // pathname is relative to dirfd
                                    let dir = read_fd(pid, dirfd)?;
                                    dir.join(pathname)
                                }
                            };
                            let interpreters = if self.args.trace_interpreter {
                                read_interpreter_recursive(&filename)
                            } else {
                                vec![]
                            };
                            p.exec_data = Some(ExecData {
                                filename,
                                argv,
                                envp,
                                cwd: read_cwd(pid)?,
                                interpreters,
                            });
                            p.preexecveat = !p.preexecveat;
                        } else if is_execveat_execve_quirk!(p.preexecveat, syscallno, regs)
                            || (!p.preexecveat && syscallno == nix::libc::SYS_execveat)
                        {
                            // execveat quirk:
                            // ------------------
                            // If execveat succeeds, in the syscall exit event, the syscall number from regs will be SYS_execve instead of SYS_execveat.
                            // and the argument registers are all zero.
                            // If execveat fails, in the syscall exit event, the syscall number from regs will still be SYS_execveat,
                            // and the argument registers are all zero.
                            let result = syscall_res_from_regs!(regs);
                            log::trace!("post execveat");
                            if self.args.successful_only && result != 0 {
                                p.exec_data = None;
                                p.preexecve = !p.preexecve;
                                ptrace_syscall(pid)?;
                                continue;
                            }
                            print_execve_trace(
                                p, result, &self.args, &self.env, &self.cwd, self.color,
                            )?;

                            p.exec_data = None;
                            // update comm
                            p.comm = read_comm(pid)?;
                            p.preexecveat = !p.preexecveat;
                        } else if syscallno == nix::libc::SYS_execve {
                            log::trace!(
                                "execve {syscallno}, preexecve: {}, preexecveat: {}",
                                p.preexecve,
                                p.preexecveat
                            );
                            if p.preexecve {
                                let filename =
                                    read_pathbuf(pid, syscall_arg!(regs, 0) as AddressType)?;
                                let argv =
                                    read_string_array(pid, syscall_arg!(regs, 1) as AddressType)?;
                                let envp =
                                    read_string_array(pid, syscall_arg!(regs, 2) as AddressType)?;
                                let interpreters = if self.args.trace_interpreter {
                                    read_interpreter_recursive(&filename)
                                } else {
                                    vec![]
                                };
                                p.exec_data = Some(ExecData {
                                    filename,
                                    argv,
                                    envp,
                                    cwd: read_cwd(pid)?,
                                    interpreters,
                                });
                                p.preexecve = !p.preexecve;
                            } else {
                                let result = syscall_res_from_regs!(regs);
                                if self.args.successful_only && result != 0 {
                                    p.exec_data = None;
                                    p.preexecve = !p.preexecve;
                                    ptrace_syscall(pid)?;
                                    continue;
                                }
                                // SAFETY: p.preexecve is false, so p.exec_data is Some
                                print_execve_trace(
                                    p, result, &self.args, &self.env, &self.cwd, self.color,
                                )?;
                                p.exec_data = None;
                                // update comm
                                p.comm = read_comm(pid)?;
                                // flip presyscall
                                p.preexecve = !p.preexecve;
                            }
                        } else if syscallno == SYS_clone || syscallno == SYS_clone3 {
                        }
                        ptrace_syscall(pid)?;
                    }
                    _ => {}
                }
            }
        } else {
            traceme()?;
            log::trace!("traceme setup!");
            if 0 != unsafe { raise(SIGSTOP) } {
                log::error!("raise failed!");
                exit(-1);
            }
            log::trace!("raise success!");
            let args = args
                .into_iter()
                .map(|s| CString::new(s))
                .collect::<Result<Vec<CString>, _>>()?;
            execvp(&args[0], &args)?;
        }
        Ok(())
    }
}
