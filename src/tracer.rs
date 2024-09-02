use std::{
  collections::BTreeMap,
  ffi::CString,
  io::{self, stdin},
  ops::ControlFlow,
  os::fd::AsRawFd,
  process::exit,
  sync::{atomic::AtomicU32, Arc, RwLock},
  time::Duration,
};

use arcstr::ArcStr;
use cfg_if::cfg_if;
use either::Either;
use enumflags2::BitFlags;
use inspect::{read_arcstr, read_output_msg_array};
use nix::{
  errno::Errno,
  libc::{
    self, dup2, pid_t, pthread_self, pthread_setname_np, raise, SYS_clone, SYS_clone3,
    AT_EMPTY_PATH, SIGSTOP, S_ISGID, S_ISUID,
  },
  sys::{
    signal::{kill, Signal},
    stat::fstat,
    wait::{waitpid, WaitPidFlag, WaitStatus},
  },
  unistd::{
    getpid, initgroups, setpgid, setresgid, setresuid, setsid, tcsetpgrp, Gid, Pid, Uid, User,
  },
};
use state::PendingDetach;
use tokio::{
  select,
  sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
};
use tracing::{debug, error, info, trace, warn};

use crate::{
  arch::{syscall_arg, syscall_no_from_regs, syscall_res_from_regs},
  cli::args::{LogModeArgs, ModifierArgs, PtraceArgs, TracerEventArgs},
  cmdbuilder::CommandBuilder,
  event::{
    filterable_event, ExecEvent, OutputMsg, ProcessStateUpdate, ProcessStateUpdateEvent,
    TracerEvent, TracerEventDetails, TracerEventDetailsKind, TracerEventMessage, TracerMessage,
  },
  printer::{Printer, PrinterArgs, PrinterOut},
  proc::{
    cached_string, diff_env, parse_envp, read_comm, read_cwd, read_exe, read_fd, read_fds,
    read_interpreter_recursive, BaselineInfo,
  },
  pty::{self, Child, UnixSlavePty},
  tracer::{inspect::read_env, state::ProcessExit},
};

use self::state::{ExecData, ProcessState, ProcessStateStore, ProcessStatus};
use self::{
  inspect::{read_string, read_string_array},
  state::BreakPoint,
};
use self::{ptrace::*, state::BreakPointStop};

mod inspect;
mod ptrace;
pub mod state;
#[cfg(test)]
mod test;

pub use inspect::InspectError;

cfg_if! {
    if #[cfg(feature = "seccomp-bpf")] {
        use crate::cli::options::SeccompBpf;
        use crate::seccomp;
        use crate::tracer::ptrace::ptrace_cont;
    }
}

pub struct Tracer {
  with_tty: bool,
  /// Sets the terminal foreground process group to the tracee
  foreground: bool,
  mode: TracerMode,
  pub store: RwLock<ProcessStateStore>,
  printer: Printer,
  modifier_args: ModifierArgs,
  filter: BitFlags<TracerEventDetailsKind>,
  baseline: Arc<BaselineInfo>,
  #[cfg(feature = "seccomp-bpf")]
  seccomp_bpf: SeccompBpf,
  msg_tx: UnboundedSender<TracerMessage>,
  user: Option<User>,
  breakpoints: RwLock<BTreeMap<u32, BreakPoint>>,
  req_tx: UnboundedSender<PendingRequest>,
  delay: Duration,
}

pub enum TracerMode {
  Tui(Option<UnixSlavePty>),
  Log,
  None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreakPointHit {
  pub bid: u32,
  pub pid: Pid,
  pub stop: BreakPointStop,
}

pub enum PendingRequest {
  ResumeProcess(BreakPointHit),
  DetachProcess {
    hit: BreakPointHit,
    signal: Option<Signal>,
    hid: u64,
  },
  #[cfg(feature = "seccomp-bpf")]
  SuspendSeccompBpf(Pid),
}

impl PartialEq for TracerMode {
  fn eq(&self, other: &Self) -> bool {
    // I think a plain match is more readable here
    #[allow(clippy::match_like_matches_macro)]
    match (self, other) {
      (Self::Log, Self::Log) => true,
      _ => false,
    }
  }
}

impl Tracer {
  // TODO: create a TracerBuilder maybe
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    mode: TracerMode,
    tracing_args: LogModeArgs,
    modifier_args: ModifierArgs,
    ptrace_args: PtraceArgs,
    tracer_event_args: TracerEventArgs,
    baseline: BaselineInfo,
    event_tx: UnboundedSender<TracerMessage>,
    user: Option<User>,
    req_tx: UnboundedSender<PendingRequest>,
  ) -> color_eyre::Result<Self> {
    let baseline = Arc::new(baseline);
    #[cfg(feature = "seccomp-bpf")]
    let seccomp_bpf = if ptrace_args.seccomp_bpf == SeccompBpf::Auto {
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
      ptrace_args.seccomp_bpf
    };
    Ok(Self {
      with_tty: match &mode {
        TracerMode::Tui(tty) => tty.is_some(),
        TracerMode::Log => true,
        TracerMode::None => true,
      },
      store: RwLock::new(ProcessStateStore::new()),
      #[cfg(feature = "seccomp-bpf")]
      seccomp_bpf,
      msg_tx: event_tx,
      user,
      filter: {
        let mut filter = tracer_event_args.filter()?;
        trace!("Event filter: {:?}", filter);
        if mode == TracerMode::Log {
          // FIXME: In logging mode, we rely on root child exit event to exit the process
          //        with the same exit code as the root child. It is not printed in logging mode.
          //        Ideally we should use another channel to send the exit code to the main thread.
          filter |= TracerEventDetailsKind::TraceeExit;
        }
        filter
      },
      printer: Printer::new(
        PrinterArgs::from_cli(&tracing_args, &modifier_args),
        baseline.clone(),
      ),
      delay: {
        let mut default = Duration::from_micros(1);
        #[cfg(feature = "seccomp-bpf")]
        if seccomp_bpf == SeccompBpf::On {
          default = Duration::from_micros(500);
        }
        ptrace_args
          .tracer_delay
          .map(Duration::from_micros)
          .unwrap_or(default)
      },
      modifier_args,
      baseline,
      mode,
      foreground: match (tracing_args.foreground, tracing_args.no_foreground) {
        (true, false) => true,
        (false, true) => false,
        // Disable foreground mode in test by default
        #[cfg(not(test))]
        _ => true,
        #[cfg(test)]
        _ => false,
      },
      breakpoints: RwLock::new(BTreeMap::new()),
      req_tx,
    })
  }

  pub fn spawn(
    self: Arc<Self>,
    args: Vec<String>,
    output: Option<Box<PrinterOut>>,
    req_rx: UnboundedReceiver<PendingRequest>,
  ) -> tokio::task::JoinHandle<color_eyre::Result<()>> {
    tokio::task::spawn_blocking({
      move || {
        unsafe {
          let current_thread = pthread_self();
          pthread_setname_np(current_thread, "tracer\0\0\0\0\0\0\0\0\0\0".as_ptr().cast());
        }
        let tx = self.msg_tx.clone();
        let result = tokio::runtime::Handle::current().block_on(async move {
          self.printer.init_thread_local(output);
          self.run(args, req_rx).await
        });
        if let Err(e) = &result {
          tx.send(TracerMessage::FatalError(e.to_string())).unwrap();
        }
        result
      }
    })
  }

  async fn run(
    self: Arc<Self>,
    args: Vec<String>,
    mut req_rx: UnboundedReceiver<PendingRequest>,
  ) -> color_eyre::Result<()> {
    trace!("start_root_process: {:?}", args);

    let mut cmd = CommandBuilder::new(&args[0]);
    cmd.args(args.iter().skip(1));
    cmd.cwd(std::env::current_dir()?);

    #[cfg(feature = "seccomp-bpf")]
    let seccomp_bpf = self.seccomp_bpf;
    let slave_pty = match &self.mode {
      TracerMode::Tui(tty) => tty.as_ref(),
      TracerMode::Log | TracerMode::None => None,
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
        trace!("traceme setup!");

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
          error!("raise failed!");
          exit(-1);
        }
        trace!("raise success!");

        Ok(())
      },
    )?
    .process_id();
    filterable_event!(TraceeSpawn(root_child)).send_if_match(&self.msg_tx, self.filter)?;
    // wait for child to be stopped by SIGSTOP
    loop {
      let status = waitpid(root_child, Some(WaitPidFlag::WSTOPPED))?;
      match status {
        WaitStatus::Stopped(_, Signal::SIGSTOP) => {
          break;
        }
        _ => {
          trace!("tracee stopped by other signal, restarting it...");
          ptrace::cont(root_child, None)?;
        }
      }
    }
    trace!("child stopped");
    let mut root_child_state = ProcessState::new(root_child, 0)?;
    root_child_state.ppid = Some(getpid());
    {
      self.store.write().unwrap().insert(root_child_state);
    }
    // Set foreground process group of the terminal
    if let TracerMode::Log | TracerMode::None = &self.mode {
      if self.foreground {
        match tcsetpgrp(stdin(), root_child) {
          Ok(_) => {}
          Err(Errno::ENOTTY) => {
            warn!("tcsetpgrp failed: ENOTTY");
          }
          r => r?,
        }
      }
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
    trace!("resuming child");
    self.seccomp_aware_cont(root_child)?;
    let mut collect_interval = tokio::time::interval(self.delay);

    loop {
      select! {
        _ = collect_interval.tick() => {
          let action = self.handle_waitpid_events(root_child)?;
          match action {
            ControlFlow::Break(_) => {
              break Ok(());
            }
            ControlFlow::Continue(_) => {}
          }
        }
        Some(req) = req_rx.recv() => {
          match req {
            PendingRequest::ResumeProcess(hit) => {
              let mut store = self.store.write().unwrap();
              let state = store.get_current_mut(hit.pid).unwrap();
              self.proprgate_operation_error(hit, true, self.resume_process(state, hit.stop))?;
            }
            PendingRequest::DetachProcess { hit, signal, hid } => {
              let mut store = self.store.write().unwrap();
              let state = store.get_current_mut(hit.pid).unwrap();
              if let Some(signal) = signal {
                if let Err(e) = self.prepare_to_detach_with_signal(state, hit, signal, hid) {
                  self.msg_tx.send(ProcessStateUpdateEvent {
                    update: ProcessStateUpdate::DetachError { hit, error: e },
                    pid: hit.pid,
                    ids: vec![],
                  }.into())?;
                }
              } else {
                self.proprgate_operation_error(hit, false, self.detach_process_internal(state, signal, hid))?;
              }
            }
            #[cfg(feature = "seccomp-bpf")]
            PendingRequest::SuspendSeccompBpf(pid) => {
              let _err = self.suspend_seccomp_bpf(pid).inspect_err(|e| {
                error!("Failed to suspend seccomp-bpf for {pid}: {e}");
              });
            }
          }
        }
      }
    }
  }

  fn handle_waitpid_events(&self, root_child: Pid) -> color_eyre::Result<ControlFlow<()>> {
    let mut counter = 0;
    loop {
      let status = waitpid(None, Some(WaitPidFlag::__WALL | WaitPidFlag::WNOHANG))?;
      if status != WaitStatus::StillAlive {
        counter += 1;
      } else {
        break;
      }
      // trace!("waitpid: {:?}", status);
      match status {
        WaitStatus::Stopped(pid, sig) => {
          trace!("stopped: {pid}, sig {:?}", sig);
          match sig {
            Signal::SIGSTOP => {
              trace!("sigstop event, child: {pid}");
              {
                let mut store = self.store.write().unwrap();
                let mut pid_reuse = false;
                let mut handled = false;
                if let Some(state) = store.get_current_mut(pid) {
                  if state.status == ProcessStatus::PtraceForkEventReceived {
                    trace!("sigstop event received after ptrace fork event, pid: {pid}");
                    state.status = ProcessStatus::Running;
                    self.seccomp_aware_cont(pid)?;
                    handled = true;
                  } else if state.status == ProcessStatus::Initialized {
                    // Manually inserted process state. (root child)
                    handled = true;
                  } else {
                    // Pid reuse
                    pid_reuse = true;
                  }
                }
                if !handled || pid_reuse {
                  trace!("sigstop event received before ptrace fork event, pid: {pid}, pid_reuse: {pid_reuse}");
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

              // The SIGCHLD can't come from other untraced subprocesses because WUNTRACED is not set.
              self.seccomp_aware_cont_with_signal(pid, Signal::SIGCHLD)?;
            }
            _ => {
              trace!("other signal: {pid}, sig {:?}", sig);
              if sig == Signal::SIGALRM {
                // Check for pending detach requests
                let mut store = self.store.write().unwrap();
                let state = store.get_current_mut(pid).unwrap();
                if let Some(detach) = state.pending_detach.take() {
                  self.proprgate_operation_error(
                    detach.hit,
                    false,
                    self.detach_process_internal(state, Some(detach.signal), detach.hid),
                  )?;
                  continue;
                }
              }
              // Just deliver the signal to tracee
              self.seccomp_aware_cont_with_signal(pid, sig)?;
            }
          }
        }
        WaitStatus::Exited(pid, code) => {
          // pid could also be a not traced subprocess.
          trace!("exited: pid {}, code {:?}", pid, code);
          let mut store = self.store.write().unwrap();
          if let Some(state) = store.get_current_mut(pid) {
            state.status = ProcessStatus::Exited(ProcessExit::Code(code));
            let mut should_exit = false;
            if pid == root_child {
              filterable_event!(TraceeExit {
                signal: None,
                exit_code: code,
              })
              .send_if_match(&self.msg_tx, self.filter)?;
              should_exit = true;
            }
            let associated_events = state.associated_events.clone();
            if !associated_events.is_empty() {
              self.msg_tx.send(
                ProcessStateUpdateEvent {
                  update: ProcessStateUpdate::Exit(ProcessExit::Code(code)),
                  pid,
                  ids: associated_events,
                }
                .into(),
              )?;
            }
            if should_exit {
              return Ok(ControlFlow::Break(()));
            }
          }
        }
        WaitStatus::PtraceEvent(pid, sig, evt) => {
          trace!("ptrace event: {:?} {:?}", sig, evt);
          match evt {
            nix::libc::PTRACE_EVENT_FORK
            | nix::libc::PTRACE_EVENT_VFORK
            | nix::libc::PTRACE_EVENT_CLONE => {
              let new_child = Pid::from_raw(ptrace::getevent(pid)? as pid_t);
              trace!("ptrace fork event, evt {evt}, pid: {pid}, child: {new_child}");
              if self.filter.intersects(TracerEventDetailsKind::NewChild) {
                let store = self.store.read().unwrap();
                let parent = store.get_current(pid).unwrap();
                let event = TracerEvent::from(TracerEventDetails::NewChild {
                  ppid: parent.pid,
                  pcomm: parent.comm.clone(),
                  pid: new_child,
                });
                self.msg_tx.send(event.into())?;
                self.printer.print_new_child(parent, new_child)?;
              }
              {
                let mut store = self.store.write().unwrap();
                let mut pid_reuse = false;
                let mut handled = false;
                if let Some(state) = store.get_current_mut(new_child) {
                  if state.status == ProcessStatus::SigstopReceived {
                    trace!(
                      "ptrace fork event received after sigstop, pid: {pid}, child: {new_child}"
                    );
                    state.status = ProcessStatus::Running;
                    state.ppid = Some(pid);
                    self.seccomp_aware_cont(new_child)?;
                    handled = true;
                  } else if state.status == ProcessStatus::Initialized {
                    // Manually inserted process state. (root child)
                    handled = true;
                  } else {
                    // Pid reuse
                    pid_reuse = true;
                  }
                }
                if !handled || pid_reuse {
                  trace!(
                        "ptrace fork event received before sigstop, pid: {pid}, child: {new_child}, pid_reuse: {pid_reuse}"
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
              trace!("exec event");
              let mut store = self.store.write().unwrap();
              let p = store.get_current_mut(pid).unwrap();
              assert!(!p.presyscall);
              // After execve or execveat, in syscall exit event,
              // the registers might be clobbered(e.g. aarch64).
              // So we need to determine whether exec is successful here.
              // PTRACE_EVENT_EXEC only happens for successful exec.
              p.is_exec_successful = true;
              // Exec event comes first before our special SIGALRM is sent to tracee! (usually happens on syscall-enter)
              if p.pending_detach.is_none() {
                // Don't use seccomp_aware_cont here because that will skip the next syscall exit stop
                self.syscall_enter_cont(pid)?;
              } else {
                self.syscall_enter_cont(pid)?;
                trace!("pending detach, continuing so that signal can be delivered");
              }
            }
            nix::libc::PTRACE_EVENT_EXIT => {
              trace!("exit event");
              self.seccomp_aware_cont(pid)?;
            }
            nix::libc::PTRACE_EVENT_SECCOMP => {
              trace!("seccomp event");
              self.on_syscall_enter(pid)?;
            }
            _ => {
              trace!("other event");
              self.seccomp_aware_cont(pid)?;
            }
          }
        }
        WaitStatus::Signaled(pid, sig, _) => {
          debug!("signaled: {pid}, {:?}", sig);
          let mut store = self.store.write().unwrap();
          if let Some(state) = store.get_current_mut(pid) {
            state.status = ProcessStatus::Exited(ProcessExit::Signal(sig));
            if pid == root_child {
              filterable_event!(TraceeExit {
                signal: Some(sig),
                exit_code: 128 + (sig as i32),
              })
              .send_if_match(&self.msg_tx, self.filter)?;
              return Ok(ControlFlow::Break(()));
            }
            let associated_events = state.associated_events.clone();
            if !associated_events.is_empty() {
              self.msg_tx.send(
                ProcessStateUpdateEvent {
                  update: ProcessStateUpdate::Exit(ProcessExit::Signal(sig)),
                  pid,
                  ids: associated_events,
                }
                .into(),
              )?;
            }
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
      if counter > 100 {
        // Give up if we have handled 100 events, so that we have a chance to handle other events
        debug!("yielding after 100 events");
        break;
      }
    }
    Ok(ControlFlow::Continue(()))
  }

  fn on_syscall_enter(&self, pid: Pid) -> color_eyre::Result<()> {
    let mut store = self.store.write().unwrap();
    let p = store.get_current_mut(pid).unwrap();
    p.presyscall = !p.presyscall;
    // SYSCALL ENTRY
    let regs = match ptrace_getregs(pid) {
      Ok(regs) => regs,
      Err(Errno::ESRCH) => {
        filterable_event!(Info(TracerEventMessage {
          msg: "Failed to read registers: ESRCH (child probably gone!)".to_string(),
          pid: Some(pid),
        }))
        .send_if_match(&self.msg_tx, self.filter)?;
        info!("ptrace getregs failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
      }
      e => e?,
    };
    let syscallno = syscall_no_from_regs!(regs);
    p.syscall = syscallno;
    // trace!("pre syscall: {syscallno}");
    if syscallno == nix::libc::SYS_execveat {
      trace!("pre execveat {syscallno}");
      // int execveat(int dirfd, const char *pathname,
      //              char *const _Nullable argv[],
      //              char *const _Nullable envp[],
      //              int flags);
      let dirfd = syscall_arg!(regs, 0) as i32;
      let flags = syscall_arg!(regs, 4) as i32;
      let filename = match read_string(pid, syscall_arg!(regs, 1) as AddressType) {
        Ok(pathname) => {
          let pathname = cached_string(pathname);
          let pathname_is_empty = pathname.is_empty();
          let filename = match (
            pathname.starts_with('/'),
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
              cached_string(format!("{dir}/{pathname}"))
            }
          };
          Ok(filename)
        }
        Err(e) => Err(e),
      };
      let filename = self.get_filename_for_display(pid, filename)?;
      self.warn_for_filename(&filename, pid)?;
      let argv = read_output_msg_array(pid, syscall_arg!(regs, 2) as AddressType);
      self.warn_for_argv(&argv, pid)?;
      let envp = read_env(pid, syscall_arg!(regs, 3) as AddressType);
      self.warn_for_envp(&envp, pid)?;

      let interpreters = if self.printer.args.trace_interpreter && filename.is_ok() {
        read_interpreter_recursive(filename.as_deref().unwrap())
      } else {
        vec![]
      };
      let filename = match filename {
        Ok(s) => OutputMsg::Ok(s),
        Err(e) => OutputMsg::Err(crate::event::FriendlyError::InspectError(e)),
      };
      p.exec_data = Some(ExecData::new(
        filename,
        argv,
        envp,
        OutputMsg::Ok(read_cwd(pid)?),
        Some(interpreters),
        read_fds(pid)?,
      ));
    } else if syscallno == nix::libc::SYS_execve {
      trace!("pre execve {syscallno}",);
      let filename = read_arcstr(pid, syscall_arg!(regs, 0) as AddressType);
      let filename = self.get_filename_for_display(pid, filename)?;
      self.warn_for_filename(&filename, pid)?;
      let argv = read_output_msg_array(pid, syscall_arg!(regs, 1) as AddressType);
      self.warn_for_argv(&argv, pid)?;
      let envp = read_string_array(pid, syscall_arg!(regs, 2) as AddressType).map(parse_envp);
      self.warn_for_envp(&envp, pid)?;
      let interpreters = if self.printer.args.trace_interpreter && filename.is_ok() {
        read_interpreter_recursive(filename.as_deref().unwrap())
      } else {
        vec![]
      };
      let filename = match filename {
        Ok(s) => OutputMsg::Ok(s),
        Err(e) => OutputMsg::Err(crate::event::FriendlyError::InspectError(e)),
      };
      p.exec_data = Some(ExecData::new(
        filename,
        argv,
        envp,
        OutputMsg::Ok(read_cwd(pid)?),
        Some(interpreters),
        read_fds(pid)?,
      ));
    } else if syscallno == SYS_clone || syscallno == SYS_clone3 {
    }
    if let Some(exec_data) = &p.exec_data {
      let mut hit = None;
      for (&idx, brk) in self
        .breakpoints
        .read()
        .unwrap()
        .iter()
        .filter(|(_, brk)| brk.activated && brk.stop == BreakPointStop::SyscallEnter)
      {
        if brk
          .pattern
          .matches(exec_data.argv.as_deref().ok(), &exec_data.filename)
        {
          hit = Some(idx);
          break;
        }
      }
      if let Some(bid) = hit {
        let associated_events = p.associated_events.clone();
        let event = ProcessStateUpdateEvent {
          update: ProcessStateUpdate::BreakPointHit(BreakPointHit {
            bid,
            pid,
            stop: BreakPointStop::SyscallEnter,
          }),
          pid,
          ids: associated_events,
        };
        p.status = ProcessStatus::BreakPointHit;
        self.msg_tx.send(event.into())?;
        return Ok(()); // Do not continue the syscall
      }
    }
    self.syscall_enter_cont(pid)?;
    Ok(())
  }

  fn on_syscall_exit(&self, pid: Pid) -> color_eyre::Result<()> {
    // SYSCALL EXIT
    // trace!("post syscall {}", p.syscall);
    let mut store = self.store.write().unwrap();
    let p = store.get_current_mut(pid).unwrap();
    p.presyscall = !p.presyscall;

    let regs = match ptrace_getregs(pid) {
      Ok(regs) => regs,
      Err(Errno::ESRCH) => {
        info!("ptrace getregs failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
      }
      e => e?,
    };
    let result = syscall_res_from_regs!(regs);
    // If exec is successful, the register value might be clobbered.
    let exec_result = if p.is_exec_successful { 0 } else { result };
    match p.syscall {
      nix::libc::SYS_execve | nix::libc::SYS_execveat => {
        trace!("post execve in exec");
        if self.printer.args.successful_only && !p.is_exec_successful {
          p.exec_data = None;
          self.seccomp_aware_cont(pid)?;
          return Ok(());
        }
        if self.filter.intersects(TracerEventDetailsKind::Exec) {
          // TODO: optimize, we don't need to collect exec event for log mode
          let event = TracerEvent::from(TracerEventDetails::Exec(Tracer::collect_exec_event(
            &self.baseline.env,
            p,
            exec_result,
          )));
          p.associate_event([event.id]);
          self.msg_tx.send(event.into())?;
          self.printer.print_exec_trace(
            p.pid,
            p.comm.clone(),
            exec_result,
            p.exec_data.as_ref().unwrap(),
            &self.baseline.env,
            &self.baseline.cwd,
          )?;
        }
        p.is_exec_successful = false;

        if let Some(exec_data) = &p.exec_data {
          let mut hit = None;
          for (&idx, brk) in self
            .breakpoints
            .read()
            .unwrap()
            .iter()
            .filter(|(_, brk)| brk.activated && brk.stop == BreakPointStop::SyscallExit)
          {
            if brk
              .pattern
              .matches(exec_data.argv.as_deref().ok(), &exec_data.filename)
            {
              hit = Some(idx);
              break;
            }
          }
          if let Some(bid) = hit {
            let associated_events = p.associated_events.clone();
            let event = ProcessStateUpdateEvent {
              update: ProcessStateUpdate::BreakPointHit(BreakPointHit {
                bid,
                pid,
                stop: BreakPointStop::SyscallExit,
              }),
              pid,
              ids: associated_events,
            };
            p.status = ProcessStatus::BreakPointHit;
            self.msg_tx.send(event.into())?;
            return Ok(()); // Do not continue the syscall
          }
        }

        p.exec_data = None;
        // update comm
        p.comm = read_comm(pid)?;
      }
      _ => (),
    }
    self.seccomp_aware_cont(pid)?;
    Ok(())
  }

  fn syscall_enter_cont(&self, pid: Pid) -> Result<(), Errno> {
    trace!("syscall enter cont: {pid}");
    ptrace_syscall(pid, None)
  }

  /// Raw syscall enter cont, without catching ESRCH
  fn raw_syscall_enter_cont(&self, pid: Pid) -> Result<(), Errno> {
    trace!("raw syscall enter cont: {pid}");
    ptrace::syscall(pid, None)
  }

  #[allow(unused)]
  fn syscall_enter_cont_with_signal(&self, pid: Pid, sig: Signal) -> Result<(), Errno> {
    trace!("syscall enter cont: {pid} with signal {sig}");
    ptrace_syscall(pid, Some(sig))
  }

  /// When seccomp-bpf is enabled, we use ptrace::cont instead of ptrace::syscall to improve performance.
  /// Then the next syscall-entry stop is skipped and the seccomp stop is used as the syscall entry stop.
  fn seccomp_aware_cont(&self, pid: Pid) -> Result<(), Errno> {
    trace!("seccomp_aware_cont: {pid}");
    #[cfg(feature = "seccomp-bpf")]
    if self.seccomp_bpf == SeccompBpf::On {
      return ptrace_cont(pid, None);
    }
    ptrace_syscall(pid, None)
  }

  fn raw_seccomp_aware_cont(&self, pid: Pid) -> Result<(), Errno> {
    trace!("raw_seccomp_aware_cont: {pid}");
    #[cfg(feature = "seccomp-bpf")]
    if self.seccomp_bpf == SeccompBpf::On {
      return ptrace::cont(pid, None);
    }
    ptrace::syscall(pid, None)
  }

  fn seccomp_aware_cont_with_signal(&self, pid: Pid, sig: Signal) -> Result<(), Errno> {
    #[cfg(feature = "seccomp-bpf")]
    if self.seccomp_bpf == SeccompBpf::On {
      return ptrace_cont(pid, Some(sig));
    }
    ptrace_syscall(pid, Some(sig))
  }

  /// Get filename for display. If the filename is /proc/self/exe, returns the actual exe path.
  fn get_filename_for_display(
    &self,
    pid: Pid,
    filename: Result<ArcStr, Errno>,
  ) -> io::Result<Result<ArcStr, Errno>> {
    if !self.modifier_args.resolve_proc_self_exe {
      return Ok(filename);
    }
    Ok(match filename {
      Ok(f) => Ok(if f == "/proc/self/exe" {
        read_exe(pid)?
      } else {
        f
      }),
      Err(e) => Err(e),
    })
  }

  fn warn_for_argv<T>(
    &self,
    argv: &Result<Vec<T>, InspectError>,
    pid: Pid,
  ) -> color_eyre::Result<()> {
    if self.filter.intersects(TracerEventDetailsKind::Warning) {
      match argv.as_deref() {
        Ok(argv) => {
          if argv.is_empty() {
            self.msg_tx.send(
              TracerEventDetails::Warning(TracerEventMessage {
                pid: Some(pid),
                msg: "Empty argv, the printed cmdline is not accurate!".to_string(),
              })
              .into_tracer_msg(),
            )?;
          }
        }
        Err(e) => {
          self.msg_tx.send(
            TracerEventDetails::Warning(TracerEventMessage {
              pid: Some(pid),
              msg: format!("Failed to read argv: {:?}", e),
            })
            .into_tracer_msg(),
          )?;
        }
      }
    }
    Ok(())
  }

  fn warn_for_envp(
    &self,
    envp: &Result<BTreeMap<OutputMsg, OutputMsg>, InspectError>,
    pid: Pid,
  ) -> color_eyre::Result<()> {
    if self.filter.intersects(TracerEventDetailsKind::Warning) {
      if let Err(e) = envp.as_ref() {
        self.msg_tx.send(
          TracerEventDetails::Warning(TracerEventMessage {
            pid: Some(pid),
            msg: format!("Failed to read envp: {:?}", e),
          })
          .into_tracer_msg(),
        )?;
      }
    }
    Ok(())
  }

  fn warn_for_filename(
    &self,
    filename: &Result<ArcStr, InspectError>,
    pid: Pid,
  ) -> color_eyre::Result<()> {
    if self.filter.intersects(TracerEventDetailsKind::Warning) {
      if let Err(e) = filename.as_deref() {
        self.msg_tx.send(
          TracerEventDetails::Warning(TracerEventMessage {
            pid: Some(pid),
            msg: format!("Failed to read filename: {:?}", e),
          })
          .into_tracer_msg(),
        )?;
      }
    }
    Ok(())
  }

  // This function does not take self due to borrow checker
  fn collect_exec_event(
    env: &BTreeMap<OutputMsg, OutputMsg>,
    state: &ProcessState,
    result: i64,
  ) -> Box<ExecEvent> {
    let exec_data = state.exec_data.as_ref().unwrap();
    Box::new(ExecEvent {
      pid: state.pid,
      cwd: exec_data.cwd.clone(),
      comm: state.comm.clone(),
      filename: exec_data.filename.clone(),
      argv: exec_data.argv.clone(),
      envp: exec_data.envp.clone(),
      interpreter: exec_data.interpreters.clone(),
      env_diff: exec_data
        .envp
        .as_ref()
        .as_ref()
        .map(|envp| diff_env(env, envp))
        .map_err(|e| *e),
      result,
      fdinfo: exec_data.fdinfo.clone(),
    })
  }
}

lazy_static::lazy_static! {
  static ref BREAKPOINT_ID: AtomicU32 = 0.into();
}

/// Breakpoint management
impl Tracer {
  pub fn add_breakpoint(&self, breakpoint: BreakPoint) -> u32 {
    let id = BREAKPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut bs = self.breakpoints.write().unwrap();
    bs.insert(id, breakpoint);
    id
  }

  pub fn replace_breakpoint(&self, id: u32, new: BreakPoint) {
    let mut bs = self.breakpoints.write().unwrap();
    if !bs.contains_key(&id) {
      panic!("Breakpoint #{id} does not exist");
    }
    bs.insert(id, new);
  }

  pub fn set_breakpoint(&self, id: u32, activated: bool) {
    let mut bs = self.breakpoints.write().unwrap();
    if let Some(b) = bs.get_mut(&id) {
      b.activated = activated;
    }
  }

  pub fn get_breakpoints(&self) -> BTreeMap<u32, BreakPoint> {
    self.breakpoints.read().unwrap().clone()
  }

  pub fn get_breakpoint_pattern_string(&self, id: u32) -> Option<String> {
    self
      .breakpoints
      .read()
      .unwrap()
      .get(&id)
      .map(|b| b.pattern.to_editable())
  }

  pub fn remove_breakpoint(&self, index: u32) {
    self.breakpoints.write().unwrap().remove(&index);
  }

  pub fn clear_breakpoints(&self) {
    self.breakpoints.write().unwrap().clear();
  }

  fn proprgate_operation_error(
    &self,
    hit: BreakPointHit,
    is_resume: bool,
    r: Result<(), Either<Errno, SendError<TracerMessage>>>,
  ) -> color_eyre::Result<()> {
    match r {
      Ok(_) => {}
      Err(Either::Left(e)) => {
        self.msg_tx.send(
          ProcessStateUpdateEvent {
            update: if is_resume {
              ProcessStateUpdate::ResumeError { hit, error: e }
            } else {
              ProcessStateUpdate::DetachError { hit, error: e }
            },
            pid: hit.pid,
            ids: vec![],
          }
          .into(),
        )?;
      }
      e => e?,
    }
    Ok(())
  }

  fn resume_process(
    &self,
    state: &mut ProcessState,
    stop: BreakPointStop,
  ) -> Result<(), Either<Errno, SendError<TracerMessage>>> {
    state.status = ProcessStatus::Running;
    if stop == BreakPointStop::SyscallEnter {
      self
        .raw_syscall_enter_cont(state.pid)
        .map_err(Either::Left)?;
    } else {
      self
        .raw_seccomp_aware_cont(state.pid)
        .map_err(Either::Left)?;
    }
    let associated_events = state.associated_events.clone();
    self
      .msg_tx
      .send(
        ProcessStateUpdateEvent {
          update: ProcessStateUpdate::Resumed,
          pid: state.pid,
          ids: associated_events,
        }
        .into(),
      )
      .map_err(Either::Right)?;
    Ok(())
  }

  fn prepare_to_detach_with_signal(
    &self,
    state: &mut ProcessState,
    hit: BreakPointHit,
    signal: Signal,
    hid: u64,
  ) -> Result<(), Errno> {
    state.pending_detach = Some(PendingDetach { signal, hid, hit });
    // Don't use *cont_with_signal because that causes
    // the loss of signal when we do it on syscall-enter.
    // Is this a kernel bug?
    kill(state.pid, Signal::SIGALRM)?;
    if hit.stop == BreakPointStop::SyscallEnter {
      self.raw_syscall_enter_cont(state.pid)?;
    } else {
      self.raw_seccomp_aware_cont(state.pid)?;
    }
    Ok(())
  }

  /// This function should only be called when in signal-delivery-stop if signal is not None. Otherwise, the signal might be ignored.
  fn detach_process_internal(
    &self,
    state: &mut ProcessState,
    signal: Option<Signal>,
    hid: u64,
  ) -> Result<(), Either<Errno, SendError<TracerMessage>>> {
    let pid = state.pid;
    trace!("detaching: {pid}, signal: {:?}", signal);
    state.status = ProcessStatus::Detached;
    ptrace::detach(pid, signal)
      .inspect_err(|e| warn!("Failed to detach from {pid}: {e}"))
      .map_err(Either::Left)?;
    trace!("detached: {pid}, signal: {:?}", signal);
    let associated_events = state.associated_events.clone();
    self
      .msg_tx
      .send(
        ProcessStateUpdateEvent {
          update: ProcessStateUpdate::Detached { hid },
          pid,
          ids: associated_events,
        }
        .into(),
      )
      .map_err(Either::Right)?;
    trace!("detach finished: {pid}, signal: {:?}", signal);
    Ok(())
  }

  pub fn request_process_detach(
    &self,
    hit: BreakPointHit,
    signal: Option<Signal>,
    hid: u64,
  ) -> color_eyre::Result<()> {
    self
      .req_tx
      .send(PendingRequest::DetachProcess { hit, signal, hid })?;
    Ok(())
  }

  pub fn request_process_resume(&self, hit: BreakPointHit) -> color_eyre::Result<()> {
    self.req_tx.send(PendingRequest::ResumeProcess(hit))?;
    Ok(())
  }

  #[cfg(feature = "seccomp-bpf")]
  fn suspend_seccomp_bpf(&self, pid: Pid) -> Result<(), Errno> {
    use nix::libc::{ptrace, PTRACE_O_SUSPEND_SECCOMP, PTRACE_SETOPTIONS};

    if self.seccomp_bpf == SeccompBpf::On {
      unsafe {
        let result = ptrace(PTRACE_SETOPTIONS, pid, 0, PTRACE_O_SUSPEND_SECCOMP);
        if -1 == result {
          let errno = Errno::last();
          error!("Failed to suspend {pid}'s seccomp filter: {errno}");
          return Err(errno);
        } else {
          trace!("suspended {pid}'s seccomp filter successfully")
        }
      }
    }
    Ok(())
  }

  #[cfg(feature = "seccomp-bpf")]
  pub fn request_suspend_seccomp_bpf(&self, pid: Pid) -> color_eyre::Result<()> {
    trace!("received request to suspend {pid}'s seccomp-bpf filter");
    self.req_tx.send(PendingRequest::SuspendSeccompBpf(pid))?;
    Ok(())
  }

  pub fn seccomp_bpf(&self) -> bool {
    cfg_if! {
      if #[cfg(feature = "seccomp-bpf")] {
        self.seccomp_bpf == SeccompBpf::On
      } else {
        false
      }
    }
  }
}
