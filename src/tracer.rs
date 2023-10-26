use std::{collections::HashMap, ffi::CString, path::PathBuf, process::exit};

use cfg_if::cfg_if;
use nix::{
    errno::Errno,
    libc::{pid_t, raise, SYS_clone, SYS_clone3, AT_EMPTY_PATH, SIGSTOP},
    sys::{
        ptrace::{self, traceme, AddressType},
        signal::Signal,
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{execvp, getpid, ForkResult, Pid},
};

use crate::{
    arch::{syscall_arg, syscall_no_from_regs, syscall_res_from_regs, PtraceRegisters},
    cli::TracingArgs,
    inspect::{read_pathbuf, read_string, read_string_array},
    printer::{print_exec_trace, print_new_child, ColorLevel, EnvPrintFormat, PrinterArgs},
    proc::{read_comm, read_cwd, read_fd, read_interpreter_recursive},
    state::{ExecData, ProcessState, ProcessStateStore, ProcessStatus},
};

pub struct Tracer {
    pub store: ProcessStateStore,
    args: PrinterArgs,
    env: HashMap<String, String>,
    cwd: std::path::PathBuf,
    print_children: bool,
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

fn ptrace_getregs(pid: Pid) -> Result<PtraceRegisters, Errno> {
    // Don't use GETREGSET on x86_64.
    // In some cases(it usually happens several times at and after exec syscall exit),
    // we only got 68/216 bytes into `regs`, which seems unreasonable. Not sure why.
    cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
            ptrace::getregs(pid)
        } else {
            let mut regs = std::mem::MaybeUninit::<PtraceRegisters>::uninit();
            let iovec = nix::libc::iovec {
                iov_base: regs.as_mut_ptr() as AddressType,
                iov_len: std::mem::size_of::<PtraceRegisters>(),
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
                return Err(errno);
            } else {
                assert_eq!(iovec.iov_len, std::mem::size_of::<PtraceRegisters>());
                unsafe { regs.assume_init() }
            };
            Ok(regs)
        }
    }
}

impl Tracer {
    pub fn new(tracing_args: TracingArgs) -> color_eyre::Result<Self> {
        Ok(Self {
            store: ProcessStateStore::new(),
            env: std::env::vars().collect(),
            cwd: std::env::current_dir()?,
            print_children: tracing_args.show_children,
            args: PrinterArgs {
                trace_comm: !tracing_args.no_show_comm,
                trace_argv: !tracing_args.no_show_argv && !tracing_args.show_cmdline,
                trace_env: match (
                    tracing_args.show_cmdline,
                    tracing_args.diff_env,
                    tracing_args.no_diff_env,
                    tracing_args.show_env,
                ) {
                    (true, ..) => EnvPrintFormat::None,
                    (false, .., true) | (false, _, true, _) => EnvPrintFormat::Raw,
                    _ => EnvPrintFormat::Diff, // diff_env is enabled by default
                },
                trace_cwd: tracing_args.show_cwd,
                print_cmdline: tracing_args.show_cmdline,
                successful_only: tracing_args.successful_only || tracing_args.show_cmdline,
                trace_interpreter: tracing_args.show_interpreter,
                trace_filename: !tracing_args.no_show_filename && !tracing_args.show_cmdline,
                decode_errno: !tracing_args.no_decode_errno,
                color: match (tracing_args.more_colors, tracing_args.less_colors) {
                    (false, false) => ColorLevel::Normal,
                    (true, false) => ColorLevel::More,
                    (false, true) => ColorLevel::Less,
                    _ => unreachable!(),
                },
            },
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
                            exit(code)
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
                                if self.print_children {
                                    let parent = self.store.get_current_mut(pid).unwrap();
                                    print_new_child(&parent, &self.args, new_child)?;
                                }
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
                                let p = self.store.get_current_mut(pid).unwrap();
                                assert!(!p.presyscall);
                                // After execve or execveat, in syscall exit event,
                                // the registers might be clobbered(e.g. aarch64).
                                // So we need to determine whether exec is successful here.
                                // PTRACE_EVENT_EXEC only happens for successful exec.
                                p.is_exec_successful = true;
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
                        if pid == root_child {
                            exit(128 + (sig as i32))
                        }
                    }
                    WaitStatus::PtraceSyscall(pid) => {
                        let p = self.store.get_current_mut(pid).unwrap();
                        if p.presyscall {
                            p.presyscall = !p.presyscall;
                            // SYSCALL ENTRY
                            let regs = match ptrace_getregs(pid) {
                                Ok(regs) => regs,
                                Err(Errno::ESRCH) => {
                                    log::info!(
                                        "ptrace getregs failed: {pid}, ESRCH, child probably gone!"
                                    );
                                    continue;
                                }
                                e => e?,
                            };
                            let syscallno = syscall_no_from_regs!(regs);
                            p.syscall = syscallno;
                            // log::trace!("pre syscall: {syscallno}");
                            if syscallno == nix::libc::SYS_execveat {
                                log::trace!("pre execveat {syscallno}");
                                // int execveat(int dirfd, const char *pathname,
                                //              char *const _Nullable argv[],
                                //              char *const _Nullable envp[],
                                //              int flags);
                                let dirfd = syscall_arg!(regs, 0) as i32;
                                let pathname =
                                    read_string(pid, syscall_arg!(regs, 1) as AddressType)?;
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
                            } else if syscallno == nix::libc::SYS_execve {
                                log::trace!("pre execve {syscallno}",);
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
                            } else if syscallno == SYS_clone || syscallno == SYS_clone3 {
                            }
                        } else {
                            // SYSCALL EXIT
                            // log::trace!("post syscall {}", p.syscall);
                            p.presyscall = !p.presyscall;

                            let regs = match ptrace_getregs(pid) {
                                Ok(regs) => regs,
                                Err(Errno::ESRCH) => {
                                    log::info!(
                                        "ptrace getregs failed: {pid}, ESRCH, child probably gone!"
                                    );
                                    continue;
                                }
                                e => e?,
                            };
                            let result = syscall_res_from_regs!(regs);
                            let exec_result = if p.is_exec_successful { 0 } else { result };
                            match p.syscall {
                                nix::libc::SYS_execve => {
                                    log::trace!("post execve in exec");
                                    if self.args.successful_only && !p.is_exec_successful {
                                        p.exec_data = None;
                                        ptrace_syscall(pid)?;
                                        continue;
                                    }
                                    // SAFETY: p.preexecve is false, so p.exec_data is Some
                                    print_exec_trace(
                                        p,
                                        exec_result,
                                        &self.args,
                                        &self.env,
                                        &self.cwd,
                                    )?;
                                    p.exec_data = None;
                                    p.is_exec_successful = false;
                                    // update comm
                                    p.comm = read_comm(pid)?;
                                }
                                nix::libc::SYS_execveat => {
                                    log::trace!("post execveat in exec");
                                    if self.args.successful_only && !p.is_exec_successful {
                                        p.exec_data = None;
                                        ptrace_syscall(pid)?;
                                        continue;
                                    }
                                    print_exec_trace(
                                        p,
                                        exec_result,
                                        &self.args,
                                        &self.env,
                                        &self.cwd,
                                    )?;
                                    p.exec_data = None;
                                    p.is_exec_successful = false;
                                    // update comm
                                    p.comm = read_comm(pid)?;
                                }
                                _ => (),
                            }
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
                .map(CString::new)
                .collect::<Result<Vec<CString>, _>>()?;
            execvp(&args[0], &args)?;
        }
        Ok(())
    }
}
