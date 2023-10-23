use std::{ffi::CString, process::exit};

use nix::{
    errno::Errno,
    libc::{pid_t, raise, SYS_clone, SYS_clone3, SIGSTOP},
    sys::{
        ptrace::{self, traceme, AddressType},
        signal::Signal,
        wait::{wait, waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{execvp, getppid, ForkResult, Pid},
};

use crate::{
    arch::{syscall_no_from_regs, syscall_res_from_regs},
    cli::TracingArgs,
    inspect::{read_cstring, read_cstring_array},
    proc::{read_argv, read_comm},
    state::{self, ExecData, ProcessState, ProcessStateStore, ProcessStatus},
};

pub struct Tracer {
    pub store: ProcessStateStore,
    args: TracingArgs,
}

impl Tracer {
    pub fn new(args: TracingArgs) -> Self {
        Self {
            store: ProcessStateStore::new(),
            args,
        }
    }

    pub fn start_root_process(&mut self, args: Vec<CString>, indent: u8) -> color_eyre::Result<()> {
        log::trace!("start_root_process: {:?}", args);
        if let ForkResult::Parent { child: root_child } = unsafe { nix::unistd::fork()? } {
            waitpid(root_child, Some(WaitPidFlag::WSTOPPED))?; // wait for child to stop
            log::trace!("child stopped");
            self.store.insert(ProcessState::new(root_child, 0, 0)?);
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
            ptrace::syscall(root_child, None)?; // restart child
            loop {
                let status = waitpid(None, Some(WaitPidFlag::__WALL | WaitPidFlag::WNOHANG))?;
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
                                        state.status = ProcessStatus::Running;
                                        ptrace::syscall(pid, None)?;
                                    } else if pid != root_child {
                                        log::error!("Unexpected SIGSTOP: {state:?}")
                                    }
                                } else {
                                    log::trace!("sigstop event received before ptrace fork event, pid: {pid}");
                                    let mut state = ProcessState::new(pid, 0, 0)?;
                                    state.status = ProcessStatus::SigstopReceived;
                                    self.store.insert(state);
                                }
                                // ptrace::syscall(pid, None)?;
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
                                ptrace::syscall(pid, None)?;
                            }
                            _ => ptrace::cont(pid, sig)?,
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
                                let new_indent = self
                                    .store
                                    .get_current_mut(pid)
                                    .ok_or(color_eyre::eyre::anyhow!("no current process"))?
                                    .indent
                                    + indent as usize;
                                if let Some(state) = self.store.get_current_mut(new_child) {
                                    if state.status == ProcessStatus::SigstopReceived {
                                        log::trace!("ptrace fork event received after sigstop, pid: {pid}, child: {new_child}");
                                        state.status = ProcessStatus::Running;
                                        state.indent = new_indent;
                                        ptrace::syscall(new_child, None)?;
                                    } else if new_child != root_child {
                                        log::error!("Unexpected fork event: {state:?}")
                                    }
                                } else {
                                    log::trace!("ptrace fork event received before sigstop, pid: {pid}, child: {new_child}");
                                    let mut state = ProcessState::new(new_child, 0, new_indent)?;
                                    state.status = ProcessStatus::PtraceForkEventReceived;
                                    self.store.insert(state);
                                }
                                // Resume parent
                                ptrace::syscall(pid, None)?;
                            }
                            nix::libc::PTRACE_EVENT_EXEC => {
                                log::trace!("exec event");
                                ptrace::syscall(pid, None)?;
                            }
                            nix::libc::PTRACE_EVENT_EXIT => {
                                log::trace!("exit event");
                                ptrace::cont(pid, None)?;
                            }
                            _ => {
                                log::trace!("other event");
                                ptrace::syscall(pid, None)?;
                            }
                        }
                    }
                    WaitStatus::Signaled(pid, sig, _) => {
                        log::trace!("signaled: {pid}, {:?}", sig);
                        // TODO: this is not correct
                        // if pid == root_child {
                        break;
                        // }
                    }
                    WaitStatus::PtraceSyscall(pid) => {
                        let regs = ptrace::getregs(pid)?;
                        let syscallno = syscall_no_from_regs!(regs);
                        let p = self.store.get_current_mut(pid).unwrap();
                        if syscallno == nix::libc::SYS_execveat {
                            log::trace!("execveat {syscallno}");
                            println!("execveat");
                        } else if syscallno == nix::libc::SYS_execve {
                            log::trace!("execve {syscallno}");
                            if p.presyscall {
                                if regs.rdi == 0 && regs.rsi == 0 && regs.rdx == 0 {
                                    // Workaround ptrace execveat quirk.
                                    // After tracing execveat, a strange execve ptrace event will happen, with PTRACE_SYSCALL_INFO_NONE.
                                    // TODO: make it less hacky.
                                    log::debug!("execveat quirk");
                                    ptrace::syscall(pid, None)?;
                                    continue;
                                }
                                let filename = read_cstring(pid, regs.rdi as AddressType)?;
                                let argv = read_cstring_array(pid, regs.rsi as AddressType)?;
                                let envp = read_cstring_array(pid, regs.rdx as AddressType)?;
                                p.exec_data = Some(ExecData {
                                    filename,
                                    argv,
                                    envp,
                                });
                                p.presyscall = !p.presyscall;
                            } else {
                                let result = syscall_res_from_regs!(regs);
                                if self.args.successful_only && result != 0 {
                                    p.exec_data = None;
                                    p.presyscall = !p.presyscall;
                                    ptrace::syscall(pid, None)?;
                                    continue;
                                }
                                // SAFETY: p.preexecve is false, so p.exec_data is Some
                                let exec_data = p.exec_data.take().unwrap();
                                let indent: String =
                                    std::iter::repeat(" ").take(p.indent).collect();
                                match (self.args.successful_only, self.args.decode_errno) {
                                    // This is very ugly, TODO: refactor
                                    (true, true) => {
                                        println!(
                                            "{}{}<{}>: {:?} {:?}",
                                            indent, pid, p.comm, exec_data.filename, exec_data.argv,
                                        );
                                    }
                                    (true, false) => {
                                        println!(
                                            "{}{}<{}>: {:?} {:?} = {}",
                                            indent,
                                            pid,
                                            p.comm,
                                            exec_data.filename,
                                            exec_data.argv,
                                            result
                                        );
                                    }
                                    (false, true) => {
                                        if result == 0 {
                                            println!(
                                                "{}{}<{}>: {:?} {:?}",
                                                indent,
                                                pid,
                                                p.comm,
                                                exec_data.filename,
                                                exec_data.argv,
                                            );
                                        } else {
                                            println!(
                                                "{}{}<{}>: {:?} {:?} = {} ({})",
                                                indent,
                                                pid,
                                                p.comm,
                                                exec_data.filename,
                                                exec_data.argv,
                                                result,
                                                nix::errno::Errno::from_i32(-result as i32)
                                            );
                                        }
                                    }
                                    (false, false) => {
                                        println!(
                                            "{}{}<{}>: {:?} {:?} = {}",
                                            indent,
                                            pid,
                                            p.comm,
                                            exec_data.filename,
                                            exec_data.argv,
                                            result
                                        );
                                    }
                                }
                                // update comm
                                p.comm = read_comm(pid)?;
                                p.presyscall = !p.presyscall;
                            }
                        } else if syscallno == SYS_clone || syscallno == SYS_clone3 {
                        }
                        ptrace::syscall(pid, None)?;
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
