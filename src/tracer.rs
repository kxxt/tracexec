use std::{
  collections::HashMap,
  ffi::CString,
  io::{self, stdin},
  os::fd::AsRawFd,
  path::PathBuf,
  process::exit,
  sync::{Arc, RwLock},
  thread::{self, JoinHandle},
};

use cfg_if::cfg_if;
use enumflags2::BitFlags;
use nix::{
  errno::Errno,
  libc::{
    self, dup2, pid_t, raise, SYS_clone, SYS_clone3, AT_EMPTY_PATH, SIGSTOP, S_ISGID, S_ISUID,
  },
  sys::{
    ptrace::{self, traceme, AddressType},
    signal::Signal,
    stat::fstat,
    wait::{waitpid, WaitPidFlag, WaitStatus},
  },
  unistd::{
    getpid, initgroups, setpgid, setresgid, setresuid, setsid, tcsetpgrp, Gid, Pid, Uid, User,
  },
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::trace;

use crate::{
  arch::{syscall_arg, syscall_no_from_regs, syscall_res_from_regs},
  cli::args::{ModifierArgs, TracerEventArgs, TracingArgs},
  cmdbuilder::CommandBuilder,
  event::{filterable_event, ExecEvent, TracerEvent, TracerEventKind, TracerMessage},
  inspect::{read_pathbuf, read_string, read_string_array, InspectError},
  printer::{Printer, PrinterArgs, PrinterOut},
  proc::{
    diff_env, read_comm, read_cwd, read_fd, read_fds, read_interpreter_recursive, BaselineInfo,
  },
  ptrace::{ptrace_getregs, ptrace_syscall},
  pty::{self, Child, UnixSlavePty},
  state::{ExecData, ProcessState, ProcessStateStore, ProcessStatus},
};

cfg_if! {
    if #[cfg(feature = "seccomp-bpf")] {
        use crate::cli::options::SeccompBpf;
        use crate::seccomp;
        use crate::ptrace::ptrace_cont;
    }

}

pub struct Tracer {
  with_tty: bool,
  mode: TracerMode,
  pub store: RwLock<ProcessStateStore>,
  printer: Printer,
  filter: BitFlags<TracerEventKind>,
  baseline: Arc<BaselineInfo>,
  #[cfg(feature = "seccomp-bpf")]
  seccomp_bpf: SeccompBpf,
  tx: UnboundedSender<TracerEvent>,
  user: Option<User>,
}

pub enum TracerMode {
  Tui(Option<UnixSlavePty>),
  Cli,
}

impl PartialEq for TracerMode {
  fn eq(&self, other: &Self) -> bool {
    // I think a plain match is more readable here
    #[allow(clippy::match_like_matches_macro)]
    match (self, other) {
      (Self::Cli, Self::Cli) => true,
      _ => false,
    }
  }
}

impl Tracer {
  pub fn new(
    mode: TracerMode,
    tracing_args: TracingArgs,
    modifier_args: ModifierArgs,
    tracer_event_args: TracerEventArgs,
    baseline: BaselineInfo,
    tx: UnboundedSender<TracerEvent>,
    user: Option<User>,
  ) -> color_eyre::Result<Self> {
    let baseline = Arc::new(baseline);
    Ok(Self {
      with_tty: match &mode {
        TracerMode::Tui(tty) => tty.is_some(),
        TracerMode::Cli => true,
      },
      store: RwLock::new(ProcessStateStore::new()),
      #[cfg(feature = "seccomp-bpf")]
      seccomp_bpf: if modifier_args.seccomp_bpf == SeccompBpf::Auto {
        // TODO: check if the kernel supports seccomp-bpf
        // Let's just enable it for now and see if anyone complains
        if user.is_some() {
          // Seccomp-bpf enforces no-new-privs, so when using --user to trace set(u|g)id
          // binaries, we disable seccomp-bpf by default.
          SeccompBpf::Off
        } else {
          SeccompBpf::On
        }
      } else {
        modifier_args.seccomp_bpf
      },
      tx,
      user,
      filter: {
        let mut filter = tracer_event_args.filter()?;
        trace!("Event filter: {:?}", filter);
        if mode == TracerMode::Cli {
          // FIXME: In logging mode, we rely on root child exit event to exit the process
          //        with the same exit code as the root child. It is not printed in logging mode.
          //        Ideally we should use another channel to send the exit code to the main thread.
          filter |= TracerEventKind::RootChildExit;
        }
        filter
      },
      printer: Printer::new(
        PrinterArgs::from_cli(&tracing_args, &modifier_args),
        baseline.clone(),
      ),
      baseline,
      mode,
    })
  }

  pub fn spawn(
    self: Arc<Self>,
    args: Vec<String>,
    output: Option<Box<PrinterOut>>,
  ) -> color_eyre::Result<JoinHandle<color_eyre::Result<()>>> {
    Ok(
      thread::Builder::new()
        .name("tracer".to_string())
        .spawn(|| {
          self.printer.init_thread_local(output);
          self.start_root_process(args)
        })?,
    )
  }

  fn start_root_process(self: Arc<Self>, args: Vec<String>) -> color_eyre::Result<()> {
    log::trace!("start_root_process: {:?}", args);

    let mut cmd = CommandBuilder::new(&args[0]);
    cmd.args(args.iter().skip(1));
    cmd.cwd(std::env::current_dir()?);

    #[cfg(feature = "seccomp-bpf")]
    let seccomp_bpf = self.seccomp_bpf;
    let slave_pty = match &self.mode {
      TracerMode::Tui(tty) => tty.as_ref(),
      TracerMode::Cli => None,
    };
    let with_tty = self.with_tty;
    let use_pseudo_term = slave_pty.is_some();
    let user = self.user.clone();

    let root_child = pty::spawn_command(
      slave_pty,
      cmd,
      |_| Ok(()),
      move |program_path| {
        #[cfg(feature = "seccomp-bpf")]
        if seccomp_bpf == SeccompBpf::On {
          let filter = seccomp::create_seccomp_filter();
          let bpf: seccompiler::BpfProgram = filter.try_into()?;
          seccompiler::apply_filter(&bpf)?;
        }

        if !with_tty {
          unsafe {
            let dev_null = std::fs::File::open("/dev/null")?;
            dup2(dev_null.as_raw_fd(), 0);
            dup2(dev_null.as_raw_fd(), 1);
            dup2(dev_null.as_raw_fd(), 2);
          }
        }

        if use_pseudo_term {
          setsid()?;
          if unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) } == -1 {
            Err(io::Error::last_os_error())?;
          }
        } else {
          let me = getpid();
          setpgid(me, me)?;
        }

        traceme()?;
        log::trace!("traceme setup!");

        if let Some(user) = &user {
          // First, read set(u|g)id info from stat
          let file = std::fs::File::open(program_path)?;
          let stat = fstat(file.as_raw_fd())?;
          drop(file);
          // setuid binary
          let euid = if stat.st_mode & S_ISUID > 0 {
            Uid::from_raw(stat.st_uid)
          } else {
            user.uid
          };
          // setgid binary
          let egid = if stat.st_mode & S_ISGID > 0 {
            Gid::from_raw(stat.st_gid)
          } else {
            user.gid
          };
          initgroups(&CString::new(user.name.as_str())?[..], user.gid)?;
          setresgid(user.gid, egid, Gid::from_raw(u32::MAX))?;
          setresuid(user.uid, euid, Uid::from_raw(u32::MAX))?;
        }

        if 0 != unsafe { raise(SIGSTOP) } {
          log::error!("raise failed!");
          exit(-1);
        }
        log::trace!("raise success!");

        Ok(())
      },
    )?
    .process_id();
    filterable_event!(RootChildSpawn(root_child)).send_if_match(&self.tx, self.filter)?;
    // wait for child to be stopped by SIGSTOP
    loop {
      let status = waitpid(root_child, Some(WaitPidFlag::WSTOPPED))?;
      match status {
        WaitStatus::Stopped(_, Signal::SIGSTOP) => {
          break;
        }
        _ => {
          log::trace!("tracee stopped by other signal, restarting it...");
          ptrace::cont(root_child, None)?;
        }
      }
    }
    log::trace!("child stopped");
    let mut root_child_state = ProcessState::new(root_child, 0)?;
    root_child_state.ppid = Some(getpid());
    {
      self.store.write().unwrap().insert(root_child_state);
    }
    // Set foreground process group of the terminal
    if let TracerMode::Cli = &self.mode {
      tcsetpgrp(stdin(), root_child)?;
    }
    let mut ptrace_opts = {
      use nix::sys::ptrace::Options;
      Options::PTRACE_O_TRACEEXEC
        | Options::PTRACE_O_TRACEEXIT
        | Options::PTRACE_O_EXITKILL
        | Options::PTRACE_O_TRACESYSGOOD
        | Options::PTRACE_O_TRACEFORK
        | Options::PTRACE_O_TRACECLONE
        | Options::PTRACE_O_TRACEVFORK
    };
    #[cfg(feature = "seccomp-bpf")]
    if self.seccomp_bpf == SeccompBpf::On {
      ptrace_opts |= ptrace::Options::PTRACE_O_TRACESECCOMP;
    }
    ptrace::setoptions(root_child, ptrace_opts)?;
    // restart child
    log::trace!("resuming child");
    self.seccomp_aware_cont(root_child)?;
    loop {
      let status = waitpid(None, Some(WaitPidFlag::__WALL))?;
      // log::trace!("waitpid: {:?}", status);
      match status {
        WaitStatus::Stopped(pid, sig) => {
          log::trace!("stopped: {pid}, sig {:?}", sig);
          match sig {
            Signal::SIGSTOP => {
              log::trace!("sigstop event, child: {pid}");
              {
                let mut store = self.store.write().unwrap();
                if let Some(state) = store.get_current_mut(pid) {
                  if state.status == ProcessStatus::PtraceForkEventReceived {
                    log::trace!("sigstop event received after ptrace fork event, pid: {pid}");
                    state.status = ProcessStatus::Running;
                    self.seccomp_aware_cont(pid)?;
                  } else if pid != root_child {
                    log::error!("Unexpected SIGSTOP: {state:?}")
                  } else {
                    log::error!("Unexpected SIGSTOP: {state:?}")
                    // let siginfo = ptrace::getsiginfo(pid)?;
                    // log::trace!(
                    //     "FIXME: this is weird, pid: {pid}, siginfo: {siginfo:?}"
                    // );
                    // let sender = siginfo._pad[1];
                    // let tmp = format!("/proc/{sender}/status");
                    // ptrace::detach(pid, Some(Signal::SIGSTOP))?;
                    // trace_dbg!(process::Command::new("/bin/cat")
                    //     .arg(tmp)
                    //     .output()?);
                    // self.seccomp_aware_cont(pid)?;
                  }
                } else {
                  log::trace!("sigstop event received before ptrace fork event, pid: {pid}");
                  let mut state = ProcessState::new(pid, 0)?;
                  state.status = ProcessStatus::SigstopReceived;
                  store.insert(state);
                }
                // https://stackoverflow.com/questions/29997244/occasionally-missing-ptrace-event-vfork-when-running-ptrace
                // DO NOT send PTRACE_SYSCALL until we receive the PTRACE_EVENT_FORK, etc.
              }
            }
            Signal::SIGCHLD => {
              // From lurk:
              //
              // The SIGCHLD signal is sent to a process when a child process terminates, interrupted, or resumes after being interrupted
              // This means, that if our tracee forked and said fork exits before the parent, the parent will get stopped.
              // Therefor issue a PTRACE_SYSCALL request to the parent to continue execution.
              // This is also important if we trace without the following forks option.
              self.seccomp_aware_cont_with_signal(pid, Signal::SIGCHLD)?;
            }
            _ => {
              // Just deliver the signal to tracee
              self.seccomp_aware_cont_with_signal(pid, sig)?;
            }
          }
        }
        WaitStatus::Exited(pid, code) => {
          log::trace!("exited: pid {}, code {:?}", pid, code);
          self
            .store
            .write()
            .unwrap()
            .get_current_mut(pid)
            .unwrap()
            .status = ProcessStatus::Exited(code);
          if pid == root_child {
            filterable_event!(RootChildExit {
              signal: None,
              exit_code: code,
            })
            .send_if_match(&self.tx, self.filter)?;
            return Ok(());
          }
        }
        WaitStatus::PtraceEvent(pid, sig, evt) => {
          log::trace!("ptrace event: {:?} {:?}", sig, evt);
          match evt {
            nix::libc::PTRACE_EVENT_FORK
            | nix::libc::PTRACE_EVENT_VFORK
            | nix::libc::PTRACE_EVENT_CLONE => {
              let new_child = Pid::from_raw(ptrace::getevent(pid)? as pid_t);
              log::trace!("ptrace fork event, evt {evt}, pid: {pid}, child: {new_child}");
              if self.filter.intersects(TracerEventKind::NewChild) {
                let store = self.store.read().unwrap();
                let parent = store.get_current(pid).unwrap();
                self.tx.send(TracerEvent::NewChild {
                  ppid: parent.pid,
                  pcomm: parent.comm.clone(),
                  pid: new_child,
                })?;
                self.printer.print_new_child(parent, new_child)?;
              }
              {
                let mut store = self.store.write().unwrap();
                if let Some(state) = store.get_current_mut(new_child) {
                  if state.status == ProcessStatus::SigstopReceived {
                    log::trace!(
                      "ptrace fork event received after sigstop, pid: {pid}, child: {new_child}"
                    );
                    state.status = ProcessStatus::Running;
                    state.ppid = Some(pid);
                    self.seccomp_aware_cont(new_child)?;
                  } else if new_child != root_child {
                    filterable_event!(Error(TracerMessage {
                    pid: Some(new_child),
                    msg: "Unexpected fork event! Please report this bug if you can provide a reproducible case.".to_string(),
                  })).send_if_match(&self.tx, self.filter)?;
                    log::error!("Unexpected fork event: {state:?}")
                  }
                } else {
                  log::trace!(
                    "ptrace fork event received before sigstop, pid: {pid}, child: {new_child}"
                  );
                  let mut state = ProcessState::new(new_child, 0)?;
                  state.status = ProcessStatus::PtraceForkEventReceived;
                  state.ppid = Some(pid);
                  store.insert(state);
                }
                // Resume parent
                self.seccomp_aware_cont(pid)?;
              }
            }
            nix::libc::PTRACE_EVENT_EXEC => {
              log::trace!("exec event");
              let mut store = self.store.write().unwrap();
              let p = store.get_current_mut(pid).unwrap();
              assert!(!p.presyscall);
              // After execve or execveat, in syscall exit event,
              // the registers might be clobbered(e.g. aarch64).
              // So we need to determine whether exec is successful here.
              // PTRACE_EVENT_EXEC only happens for successful exec.
              p.is_exec_successful = true;
              // Don't use seccomp_aware_cont here because that will skip the next syscall exit stop
              self.syscall_enter_cont(pid)?;
            }
            nix::libc::PTRACE_EVENT_EXIT => {
              log::trace!("exit event");
              self.seccomp_aware_cont(pid)?;
            }
            nix::libc::PTRACE_EVENT_SECCOMP => {
              log::trace!("seccomp event");
              self.on_syscall_enter(pid)?;
            }
            _ => {
              log::trace!("other event");
              self.seccomp_aware_cont(pid)?;
            }
          }
        }
        WaitStatus::Signaled(pid, sig, _) => {
          // TODO: replace log
          log::debug!("signaled: {pid}, {:?}", sig);
          if pid == root_child {
            filterable_event!(RootChildExit {
              signal: Some(sig),
              exit_code: 128 + (sig as i32),
            })
            .send_if_match(&self.tx, self.filter)?;
            return Ok(());
          }
        }
        WaitStatus::PtraceSyscall(pid) => {
          let presyscall = self
            .store
            .write()
            .unwrap()
            .get_current_mut(pid)
            .unwrap()
            .presyscall;
          if presyscall {
            self.on_syscall_enter(pid)?;
          } else {
            self.on_syscall_exit(pid)?;
          }
        }
        _ => {}
      }
    }
  }

  fn on_syscall_enter(&self, pid: Pid) -> color_eyre::Result<()> {
    let mut store = self.store.write().unwrap();
    let p = store.get_current_mut(pid).unwrap();
    p.presyscall = !p.presyscall;
    // SYSCALL ENTRY
    let regs = match ptrace_getregs(pid) {
      Ok(regs) => regs,
      Err(Errno::ESRCH) => {
        filterable_event!(Info(TracerMessage {
          msg: "Failed to read registers: ESRCH (child probably gone!)".to_string(),
          pid: Some(pid),
        }))
        .send_if_match(&self.tx, self.filter)?;
        log::info!("ptrace getregs failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
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
      let flags = syscall_arg!(regs, 4) as i32;
      let filename = match read_string(pid, syscall_arg!(regs, 1) as AddressType) {
        Ok(pathname) => {
          let pathname_is_empty = pathname.is_empty();
          let pathname = PathBuf::from(pathname);
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
          Ok(filename)
        }
        Err(e) => Err(e),
      };
      self.warn_for_filename(&filename, pid)?;
      let argv = read_string_array(pid, syscall_arg!(regs, 2) as AddressType);
      self.warn_for_argv(&argv, pid)?;
      let envp = read_string_array(pid, syscall_arg!(regs, 3) as AddressType);
      self.warn_for_envp(&envp, pid)?;

      let interpreters = if self.printer.args.trace_interpreter && filename.is_ok() {
        read_interpreter_recursive(filename.as_deref().unwrap())
      } else {
        vec![]
      };
      p.exec_data = Some(ExecData::new(
        filename,
        argv,
        envp,
        read_cwd(pid)?,
        interpreters,
        read_fds(pid)?,
      ));
    } else if syscallno == nix::libc::SYS_execve {
      log::trace!("pre execve {syscallno}",);
      let filename = read_pathbuf(pid, syscall_arg!(regs, 0) as AddressType);
      self.warn_for_filename(&filename, pid)?;
      let argv = read_string_array(pid, syscall_arg!(regs, 1) as AddressType);
      self.warn_for_argv(&argv, pid)?;
      let envp = read_string_array(pid, syscall_arg!(regs, 2) as AddressType);
      self.warn_for_envp(&envp, pid)?;
      let interpreters = if self.printer.args.trace_interpreter && filename.is_ok() {
        read_interpreter_recursive(filename.as_deref().unwrap())
      } else {
        vec![]
      };
      p.exec_data = Some(ExecData::new(
        filename,
        argv,
        envp,
        read_cwd(pid)?,
        interpreters,
        read_fds(pid)?,
      ));
    } else if syscallno == SYS_clone || syscallno == SYS_clone3 {
    }
    self.syscall_enter_cont(pid)?;
    Ok(())
  }

  fn on_syscall_exit(&self, pid: Pid) -> color_eyre::Result<()> {
    // SYSCALL EXIT
    // log::trace!("post syscall {}", p.syscall);
    let mut store = self.store.write().unwrap();
    let p = store.get_current_mut(pid).unwrap();
    p.presyscall = !p.presyscall;

    let regs = match ptrace_getregs(pid) {
      Ok(regs) => regs,
      Err(Errno::ESRCH) => {
        log::info!("ptrace getregs failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
      }
      e => e?,
    };
    let result = syscall_res_from_regs!(regs);
    // If exec is successful, the register value might be clobbered.
    let exec_result = if p.is_exec_successful { 0 } else { result };
    match p.syscall {
      nix::libc::SYS_execve => {
        log::trace!("post execve in exec");
        if self.printer.args.successful_only && !p.is_exec_successful {
          p.exec_data = None;
          self.seccomp_aware_cont(pid)?;
          return Ok(());
        }
        if self.filter.intersects(TracerEventKind::Exec) {
          // TODO: optimize, we don't need to collect exec event for log mode
          self.tx.send(TracerEvent::Exec(Tracer::collect_exec_event(
            &self.baseline.env,
            p,
            exec_result,
          )))?;
          self
            .printer
            .print_exec_trace(p, exec_result, &self.baseline.env, &self.baseline.cwd)?;
        }
        p.exec_data = None;
        p.is_exec_successful = false;
        // update comm
        p.comm = read_comm(pid)?;
      }
      nix::libc::SYS_execveat => {
        log::trace!("post execveat in exec");
        if self.printer.args.successful_only && !p.is_exec_successful {
          p.exec_data = None;
          self.seccomp_aware_cont(pid)?;
          return Ok(());
        }
        if self.filter.intersects(TracerEventKind::Exec) {
          self.tx.send(TracerEvent::Exec(Tracer::collect_exec_event(
            &self.baseline.env,
            p,
            exec_result,
          )))?;
          self
            .printer
            .print_exec_trace(p, exec_result, &self.baseline.env, &self.baseline.cwd)?;
        }
        p.exec_data = None;
        p.is_exec_successful = false;
        // update comm
        p.comm = read_comm(pid)?;
      }
      _ => (),
    }
    self.seccomp_aware_cont(pid)?;
    Ok(())
  }

  fn syscall_enter_cont(&self, pid: Pid) -> Result<(), Errno> {
    ptrace_syscall(pid, None)
  }

  /// When seccomp-bpf is enabled, we use ptrace::cont instead of ptrace::syscall to improve performance.
  /// Then the next syscall-entry stop is skipped and the seccomp stop is used as the syscall entry stop.
  fn seccomp_aware_cont(&self, pid: Pid) -> Result<(), Errno> {
    #[cfg(feature = "seccomp-bpf")]
    if self.seccomp_bpf == SeccompBpf::On {
      return ptrace_cont(pid, None);
    }
    ptrace_syscall(pid, None)
  }

  fn seccomp_aware_cont_with_signal(&self, pid: Pid, sig: Signal) -> Result<(), Errno> {
    #[cfg(feature = "seccomp-bpf")]
    if self.seccomp_bpf == SeccompBpf::On {
      return ptrace_cont(pid, Some(sig));
    }
    ptrace_syscall(pid, Some(sig))
  }

  fn warn_for_argv(
    &self,
    argv: &Result<Vec<String>, InspectError>,
    pid: Pid,
  ) -> color_eyre::Result<()> {
    if self.filter.intersects(TracerEventKind::Warning) {
      match argv.as_deref() {
        Ok(argv) => {
          if argv.is_empty() {
            self.tx.send(TracerEvent::Warning(TracerMessage {
              pid: Some(pid),
              msg: "Empty argv, the printed cmdline is not accurate!".to_string(),
            }))?;
          }
        }
        Err(e) => {
          self.tx.send(TracerEvent::Warning(TracerMessage {
            pid: Some(pid),
            msg: format!("Failed to read argv: {:?}", e),
          }))?;
        }
      }
    }
    Ok(())
  }

  fn warn_for_envp(
    &self,
    envp: &Result<Vec<String>, InspectError>,
    pid: Pid,
  ) -> color_eyre::Result<()> {
    if self.filter.intersects(TracerEventKind::Warning) {
      if let Err(e) = envp.as_deref() {
        self.tx.send(TracerEvent::Warning(TracerMessage {
          pid: Some(pid),
          msg: format!("Failed to read envp: {:?}", e),
        }))?;
      }
    }
    Ok(())
  }

  fn warn_for_filename(
    &self,
    filename: &Result<PathBuf, InspectError>,
    pid: Pid,
  ) -> color_eyre::Result<()> {
    if self.filter.intersects(TracerEventKind::Warning) {
      if let Err(e) = filename.as_deref() {
        self.tx.send(TracerEvent::Warning(TracerMessage {
          pid: Some(pid),
          msg: format!("Failed to read filename: {:?}", e),
        }))?;
      }
    }
    Ok(())
  }

  // This function does not take self due to borrow checker
  fn collect_exec_event(
    env: &HashMap<String, String>,
    state: &ProcessState,
    result: i64,
  ) -> Box<ExecEvent> {
    let exec_data = state.exec_data.as_ref().unwrap();
    Box::new(ExecEvent {
      pid: state.pid,
      cwd: exec_data.cwd.to_owned(),
      comm: state.comm.clone(),
      filename: exec_data.filename.clone(),
      argv: exec_data.argv.clone(),
      envp: exec_data.envp.clone(),
      interpreter: exec_data.interpreters.clone(),
      env_diff: exec_data
        .envp
        .as_deref()
        .map(|envp| diff_env(env, envp))
        .map_err(|e| *e),
      result,
    })
  }
}
