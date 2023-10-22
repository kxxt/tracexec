use std::{ffi::CString, process::exit};

use nix::{
    libc::{raise, SIGSTOP},
    sys::{
        ptrace::{self, traceme, AddressType},
        signal::Signal,
        wait::{wait, waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{execvp, ForkResult},
};

use crate::{
    cli::TracingArgs,
    inspect::{read_cstring, read_cstring_array},
    proc::read_argv,
    state::{ExecData, ProcessState, ProcessStateStore, ProcessStatus},
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

    pub fn start_root_process(&mut self, args: Vec<CString>) -> color_eyre::Result<()> {
        log::trace!("start_root_process: {:?}", args);
        if let ForkResult::Parent { child: root_child } = unsafe { nix::unistd::fork()? } {
            waitpid(root_child, Some(WaitPidFlag::WSTOPPED))?; // wait for child to stop
            log::trace!("child stopped");
            self.store.insert(ProcessState {
                pid: root_child,
                status: ProcessStatus::Running,
                start_time: 0,
                command: args.clone(),
                preexecve: true,
                exec_data: None,
            });
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
                let status = wait()?;

                match status {
                    WaitStatus::Stopped(pid, sig) => {
                        log::trace!("stopped: {pid}, sig {:?}", sig);
                        match sig {
                            Signal::SIGSTOP => {
                                log::trace!("fork event, child: {pid}");
                                self.store.insert(ProcessState {
                                    pid: pid,
                                    status: ProcessStatus::Running,
                                    start_time: 0,
                                    command: read_argv(pid)?,
                                    preexecve: true,
                                    exec_data: None,
                                });
                                ptrace::syscall(pid, None)?;
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
                        log::trace!("exited: {:?}", code);
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
                            | nix::libc::PTRACE_EVENT_CLONE => {}
                            nix::libc::PTRACE_EVENT_EXEC => {
                                log::trace!("exec event");
                            }
                            nix::libc::PTRACE_EVENT_EXIT => {
                                log::trace!("exit event");
                            }
                            _ => {
                                log::trace!("other event");
                            }
                        }
                        ptrace::syscall(pid, None)?;
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
                        let syscallno = regs.orig_rax as i64;
                        let p = self.store.get_current_mut(pid).unwrap();
                        if syscallno == nix::libc::SYS_execve {
                            if p.preexecve {
                                let filename = read_cstring(pid, regs.rdi as AddressType)?;
                                let argv = read_cstring_array(pid, regs.rsi as AddressType)?;
                                let envp = read_cstring_array(pid, regs.rdx as AddressType)?;
                                p.exec_data = Some(ExecData {
                                    filename,
                                    argv,
                                    envp,
                                });
                                p.preexecve = !p.preexecve;
                            } else {
                                let result = regs.rax as i64;
                                if self.args.successful_only && result != 0 {
                                    p.exec_data = None;
                                    p.preexecve = !p.preexecve;
                                    ptrace::syscall(pid, None)?;
                                    continue;
                                }
                                // SAFETY: p.preexecve is false, so p.exec_data is Some
                                let exec_data = p.exec_data.take().unwrap();

                                println!(
                                    "{}: {:?} {:?} = {}",
                                    pid, exec_data.filename, exec_data.argv, result
                                );
                                p.preexecve = !p.preexecve;
                            }
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
                log::trace!("raise failed!");
                exit(-1);
            }
            log::trace!("raise success!");
            log::trace!("executing...");
            let args = args
                .into_iter()
                .map(|s| CString::new(s))
                .collect::<Result<Vec<CString>, _>>()?;
            execvp(&args[0], &args)?;
        }
        Ok(())
    }
}
