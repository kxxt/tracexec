use std::{
  collections::{BTreeMap, HashMap},
  fs::File,
  io::{self, Read, Write, stdin},
  ops::ControlFlow,
  os::fd::{AsRawFd, FromRawFd, OwnedFd},
  sync::{Arc, RwLock, atomic::AtomicU32},
  time::Duration,
};

use crate::{
  cache::ArcStr,
  event::ParentEventId,
  otlp::tracer::OtlpTracer,
  timestamp::Timestamp,
  tracee,
  tracer::{ExecData, ProcessExit, TracerBuilder, TracerMode},
};
use chrono::Local;
use either::Either;
use enumflags2::BitFlags;
use inspect::{read_arcstr, read_output_msg_array};
use nix::{
  errno::Errno,
  libc::{self, AT_EMPTY_PATH, S_ISGID, S_ISUID, c_int, pthread_self, pthread_setname_np},
  sys::{ptrace::AddressType, stat::fstat, wait::WaitPidFlag},
  unistd::{Gid, Pid, Uid, User, getpid, tcsetpgrp},
};
use state::{PendingDetach, Syscall};
use tokio::{
  select,
  sync::{
    mpsc::{UnboundedReceiver, UnboundedSender, error::SendError, unbounded_channel},
    oneshot,
  },
  task::JoinHandle,
};
use tracing::{debug, error, info, trace, warn};

use crate::{
  arch::RegsExt,
  cli::args::ModifierArgs,
  cmdbuilder::CommandBuilder,
  event::{
    ExecEvent, OutputMsg, ProcessStateUpdate, ProcessStateUpdateEvent, TracerEvent,
    TracerEventDetails, TracerEventDetailsKind, TracerEventMessage, TracerMessage,
    filterable_event,
  },
  printer::{Printer, PrinterOut},
  proc::{
    BaselineInfo, cached_string, diff_env, parse_envp, read_comm, read_cwd, read_exe, read_fd,
    read_fds, read_interpreter_recursive,
  },
  ptrace::inspect::{self, read_env},
  ptrace::{
    PtraceSeccompStopGuard, PtraceSignalDeliveryStopGuard, PtraceStop, PtraceStopGuard,
    PtraceSyscallLikeStop, PtraceSyscallStopGuard, PtraceWaitPidEvent, RecursivePtraceEngine,
    Signal,
  },
  pty,
};

use self::inspect::{read_string, read_string_array};
use self::state::{ProcessState, ProcessStateStore, ProcessStatus};
use super::breakpoint::{BreakPoint, BreakPointStop};

mod state;
#[cfg(test)]
mod test;

use inspect::InspectError;

use super::BreakPointHit;

use crate::cli::options::SeccompBpf;
use crate::seccomp;

/// PTRACE tracer implementation.
///
/// The public api is Sync but internal implementation uses a dedicated
/// tokio blocking thread which uses !Sync data structures.
///
/// Implementation wise, The [`Tracer`]` is `!Send` once it is running.
/// However, when it has not started yet, we can move it to another thread.
/// (In `spawn` with a Send wrapper)
///
/// But from a user's perspective, [`TracerBuilder::build_ptrace`] returns a
/// [`SpawnToken`] to restrict that a tracer can only spawn once. And the user
/// can call the public API of [`Tracer`] on arbitrary thread.
/// So [`Arc<Tracer>`] should be [`Send`].
#[derive(Debug)]
pub struct Tracer {
  with_tty: bool,
  mode: TracerMode,
  pub store: RwLock<ProcessStateStore>,
  printer: Printer,
  modifier_args: ModifierArgs,
  filter: BitFlags<TracerEventDetailsKind>,
  baseline: Arc<BaselineInfo>,
  seccomp_bpf: SeccompBpf,
  msg_tx: UnboundedSender<TracerMessage>,
  user: Option<User>,
  breakpoints: RwLock<BTreeMap<u32, BreakPoint>>,
  req_tx: UnboundedSender<PendingRequest>,
  delay: Duration,
  otlp: OtlpTracer,
}

unsafe impl Sync for Tracer {}

pub struct SpawnToken {
  req_rx: UnboundedReceiver<PendingRequest>,
  /// The tx part is only used to check if this token belongs
  /// to the same [`Tracer`] where it comes from.
  req_tx: UnboundedSender<PendingRequest>,
}

impl TracerBuilder {
  pub fn build_ptrace(self) -> color_eyre::Result<(Tracer, SpawnToken)> {
    let seccomp_bpf = if self.seccomp_bpf == SeccompBpf::Auto {
      // TODO: check if the kernel supports seccomp-bpf
      // Let's just enable it for now and see if anyone complains
      if self.user.is_some() {
        // Seccomp-bpf enforces no-new-privs, so when using --user to trace set(u|g)id
        // binaries, we disable seccomp-bpf by default.
        SeccompBpf::Off
      } else {
        SeccompBpf::On
      }
    } else {
      self.seccomp_bpf
    };
    let with_tty = match self.mode.as_ref().unwrap() {
      TracerMode::Tui(tty) => tty.is_some(),
      TracerMode::Log { .. } => true,
    };
    let (req_tx, req_rx) = unbounded_channel();
    Ok((
      Tracer {
        with_tty,
        store: RwLock::new(ProcessStateStore::new()),
        seccomp_bpf,
        msg_tx: self.tx.expect("tracer_tx is required for ptrace tracer"),
        user: self.user,
        printer: self.printer.unwrap(),
        modifier_args: self.modifier,
        filter: {
          let mut filter = self
            .filter
            .unwrap_or_else(BitFlags::<TracerEventDetailsKind>::all);
          trace!("Event filter: {:?}", filter);
          if let TracerMode::Log { .. } = self.mode.as_ref().unwrap() {
            // FIXME: In logging mode, we rely on root child exit event to exit the process
            //        with the same exit code as the root child. It is not printed in logging mode.
            //        Ideally we should use another channel to send the exit code to the main thread.
            filter |= TracerEventDetailsKind::TraceeExit;
          }
          filter
        },
        baseline: self.baseline.unwrap(),
        breakpoints: RwLock::new(BTreeMap::new()),
        req_tx: req_tx.clone(),
        delay: {
          let default = if seccomp_bpf == SeccompBpf::On {
            Duration::from_micros(500)
          } else {
            Duration::from_micros(1)
          };
          self
            .ptrace_polling_delay
            .map(Duration::from_micros)
            .unwrap_or(default)
        },
        mode: self.mode.unwrap(),
        otlp: self.otlp,
      },
      SpawnToken { req_rx, req_tx },
    ))
  }
}

pub enum PendingRequest {
  ResumeProcess(BreakPointHit),
  DetachProcess {
    hit: BreakPointHit,
    signal: Option<Signal>,
    hid: u64,
  },
  SuspendSeccompBpf(Pid),
}

impl Tracer {
  pub fn spawn(
    self: Arc<Self>,
    args: Vec<String>,
    output: Option<Box<PrinterOut>>,
    token: SpawnToken,
  ) -> JoinHandle<color_eyre::Result<()>> {
    if !self.req_tx.same_channel(&token.req_tx) {
      panic!("The spawn token used does not match the tracer")
    }
    drop(token.req_tx);
    #[derive(Debug)]
    struct SendWrapper<T>(T);
    unsafe impl<T> Send for SendWrapper<T> {}
    let this = SendWrapper(self);
    let (tx, rx) = oneshot::channel();
    tx.send(this).unwrap();
    tokio::task::spawn_blocking({
      move || {
        let this = rx.blocking_recv()?.0;
        unsafe {
          let current_thread = pthread_self();
          pthread_setname_np(current_thread, c"tracer".as_ptr());
        }
        let tx = this.msg_tx.clone();
        let result = tokio::runtime::Handle::current().block_on(async move {
          this.printer.init_thread_local(output);
          this.run(args, token.req_rx).await
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

    let seccomp_bpf = self.seccomp_bpf;
    let slave_pty = match &self.mode {
      TracerMode::Tui(tty) => tty.as_ref(),
      TracerMode::Log { .. } => None,
    };
    let with_tty = self.with_tty;
    let use_pseudo_term = slave_pty.is_some();
    let user = self.user.clone();

    let mut fds: [c_int; 2] = [0; 2];
    let ret = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if ret != 0 {
      return Err(Errno::last().into());
    }
    let tracee_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let mut tracer_fd = unsafe { File::from_raw_fd(fds[1]) };
    let tracee_raw_fd = tracee_fd.as_raw_fd();
    let root_child = pty::spawn_command(slave_pty, cmd, move |program_path| {
      if seccomp_bpf == SeccompBpf::On {
        seccomp::load_seccomp_filters()?;
      }

      if !with_tty {
        tracee::nullify_stdio()?;
      }

      if use_pseudo_term {
        tracee::lead_session_and_control_terminal()?;
      } else {
        tracee::lead_process_group()?;
      }

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
        tracee::runas(user, Some((euid, egid)))?;
      }

      trace!("Waiting for tracer");
      let mut tracee_fd = unsafe { File::from_raw_fd(tracee_raw_fd) };
      let mut message = [0; 2];
      tracee_fd.read_exact(&mut message)?;
      trace!("tracee continue to exec");

      Ok(())
    })?;
    filterable_event!(TraceeSpawn {
      timestamp: Local::now(),
      pid: root_child
    })
    .send_if_match(&self.msg_tx, self.filter)?;
    drop(tracee_fd);
    let ptrace_opts = {
      use nix::sys::ptrace::Options;
      Options::PTRACE_O_TRACEEXEC | Options::PTRACE_O_EXITKILL | Options::PTRACE_O_TRACESYSGOOD
    };
    let mut engine = RecursivePtraceEngine::new(self.seccomp_bpf());
    engine.seize_children_recursive(root_child, ptrace_opts)?;
    let mut root_child_state = ProcessState::new(root_child)?;
    root_child_state.ppid = Some(getpid());
    {
      self.store.write().unwrap().insert(root_child_state);
    }
    // Set foreground process group of the terminal
    if matches!(&self.mode, TracerMode::Log { foreground: true }) {
      match tcsetpgrp(stdin(), root_child) {
        Ok(_) => {}
        Err(Errno::ENOTTY) => {
          warn!("tcsetpgrp failed: ENOTTY");
        }
        r => r?,
      }
    }

    // Resume tracee
    // Write a message of exactly two bytes to wake up tracee to proceed to exec
    tracer_fd.write_all(b"go")?;
    drop(tracer_fd);
    trace!("Wrote message to tracee");

    let mut collect_interval = tokio::time::interval(self.delay);
    let mut pending_guards = HashMap::new();

    loop {
      select! {
        _ = collect_interval.tick() => {
          let action = self.handle_waitpid_events(&engine, root_child, &mut pending_guards)?;
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
              self.proprgate_operation_error(hit, true, self.resume_process(state, hit.stop, &mut pending_guards))?;
            }
            PendingRequest::DetachProcess { hit, signal, hid } => {
              let mut store = self.store.write().unwrap();
              let state = store.get_current_mut(hit.pid).unwrap();
              if let Some(signal) = signal {
                if let Err(e) = self.prepare_to_detach_with_signal(state, hit, signal, hid, &mut pending_guards) {
                  self.msg_tx.send(ProcessStateUpdateEvent {
                    update: ProcessStateUpdate::DetachError { hit, error: e },
                    pid: hit.pid,
                    ids: vec![],
                  }.into())?;
                }
              } else {
                self.proprgate_operation_error(hit, false, self.detach_process_internal(state, None, hid, &mut pending_guards))?;
              }
            }
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

  fn handle_waitpid_events<'a>(
    &self,
    engine: &'a RecursivePtraceEngine,
    root_child: Pid,
    pending_guards: &mut HashMap<Pid, PtraceStopGuard<'a>>,
  ) -> color_eyre::Result<ControlFlow<()>> {
    let mut counter = 0;
    loop {
      let status = engine.next_event(Some(WaitPidFlag::__WALL | WaitPidFlag::WNOHANG))?;
      if !matches!(status, PtraceWaitPidEvent::StillAlive) {
        counter += 1;
      }
      // trace!("waitpid: {:?}", status);
      match status {
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Syscall(guard)) => {
          let presyscall = self
            .store
            .write()
            .unwrap()
            .get_current_mut(guard.pid())
            .unwrap()
            .presyscall;
          if presyscall {
            self.on_syscall_enter(Either::Left(guard), pending_guards)?;
          } else {
            self.on_syscall_exit(guard, pending_guards)?;
          }
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Seccomp(guard)) => {
          self.on_syscall_enter(Either::Right(guard), pending_guards)?;
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::SignalDelivery(guard)) => {
          if guard.signal() == SENTINEL_SIGNAL {
            let mut store = self.store.write().unwrap();
            if let Some(state) = store.get_current_mut(guard.pid()) {
              if let Some(detach) = state.pending_detach.take() {
                // This is a sentinel signal
                self.proprgate_operation_error(
                  detach.hit,
                  false,
                  self.detach_process_internal(
                    state,
                    Some((detach.signal, guard)),
                    detach.hid,
                    pending_guards,
                  ),
                )?;
                continue;
              }
            }
          }
          let signal = guard.signal();
          let pid = guard.pid();
          trace!("other signal: {pid}, sig {:?}", signal);
          // Just deliver the signal to tracee
          guard.seccomp_aware_deliver_cont_syscall(true)?;
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Exec(guard)) => {
          trace!("exec event");
          let pid = guard.pid();
          let mut store = self.store.write().unwrap();
          let p = store.get_current_mut(pid).unwrap();
          assert!(!p.presyscall);
          // After execve or execveat, in syscall exit event,
          // the registers might be clobbered(e.g. aarch64).
          // So we need to determine whether exec is successful here.
          // PTRACE_EVENT_EXEC only happens for successful exec.
          p.is_exec_successful = true;
          // Exec event comes first before our special SENTINEL_SIGNAL is sent to tracee! (usually happens on syscall-enter)
          if p.pending_detach.is_none() {
            // Don't use seccomp_aware_cont here because that will skip the next syscall exit stop
            guard.cont_syscall(true)?;
          } else {
            guard.cont_syscall(true)?;
            trace!("pending detach, continuing so that signal can be delivered");
          }
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Exit(_)) => unreachable!(),
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneChild(guard)) => {
          let pid = guard.pid();
          trace!("sigstop event, child: {pid}");
          {
            let mut store = self.store.write().unwrap();
            let mut pid_reuse = false;
            let mut handled = false;
            if let Some(state) = store.get_current_mut(pid) {
              // This pid is already tracked.
              if state.status == ProcessStatus::PtraceForkEventReceived {
                trace!("sigstop event received after ptrace fork event, pid: {pid}");
                state.status = ProcessStatus::Running;
                guard.seccomp_aware_cont_syscall(true)?;
                handled = true;
              } else if state.status == ProcessStatus::Initialized {
                // Manually inserted process state. (root child)
                handled = true;
              } else if matches!(state.status, ProcessStatus::Exited(_)) {
                // Pid reuse
                pid_reuse = true;
                pending_guards.insert(pid, guard.into());
              } else {
                handled = true;
                trace!("bogus clone child event, ignoring");
                guard.seccomp_aware_cont_syscall(true)?;
              }
            } else {
              pending_guards.insert(pid, guard.into());
            }
            // Either this is an untracked new progress, or pid_reuse happened
            if !handled || pid_reuse {
              trace!(
                "sigstop event received before ptrace fork event, pid: {pid}, pid_reuse: {pid_reuse}"
              );
              let mut state = ProcessState::new(pid)?;
              state.status = ProcessStatus::SigstopReceived;
              store.insert(state);

              // https://stackoverflow.com/questions/29997244/occasionally-missing-ptrace-event-vfork-when-running-ptrace
              // DO NOT send PTRACE_SYSCALL until we receive the PTRACE_EVENT_FORK, etc.
            }
          }
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::CloneParent(guard)) => {
          let timestamp = Local::now();
          let new_child = guard.child()?;
          let pid = guard.pid();
          trace!("ptrace fork/clone event, pid: {pid}, child: {new_child}");
          if self.filter.intersects(TracerEventDetailsKind::NewChild) {
            let store = self.store.read().unwrap();
            let parent = store.get_current(pid).unwrap();
            let event = TracerEvent::from(TracerEventDetails::NewChild {
              timestamp,
              ppid: parent.pid,
              pcomm: parent.comm.clone(),
              pid: new_child,
            });
            self.msg_tx.send(event.into())?;
            self
              .printer
              .print_new_child(parent.pid, &parent.comm, new_child)?;
          }
          {
            let mut store = self.store.write().unwrap();
            let mut pid_reuse = false;
            let mut handled = false;
            if let Some(state) = store.get_current_mut(new_child) {
              if state.status == ProcessStatus::SigstopReceived {
                trace!("ptrace fork event received after sigstop, pid: {pid}, child: {new_child}");
                state.status = ProcessStatus::Running;
                state.ppid = Some(pid);
                pending_guards
                  .remove(&new_child)
                  .unwrap()
                  .seccomp_aware_cont_syscall(true)?;
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
              let mut state = ProcessState::new(new_child)?;
              state.status = ProcessStatus::PtraceForkEventReceived;
              state.ppid = Some(pid);
              store.insert(state);
            }
            let [parent_s, child_s] = store.get_current_disjoint_mut(pid, new_child);
            let parent_s = parent_s.unwrap();
            if let Some(state) = child_s {
              // We need to keep track of the parent event id of the exec event of the forked child
              // Here, we record the last exec event id of the parent process for this child.
              state
                .parent_tracker
                .save_parent_last_exec(&parent_s.parent_tracker);
              debug!(
                "save parent last exec for {new_child}, parent = {pid}, curr = {:?}, par = {:?}",
                state.parent_tracker, parent_s.parent_tracker
              );
            }
            // Resume parent
            guard.seccomp_aware_cont_syscall(true)?;
          }
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Group(guard)) => {
          guard.listen(true)?;
        }
        PtraceWaitPidEvent::Ptrace(PtraceStopGuard::Interrupt(guard)) => {
          guard.seccomp_aware_cont_syscall(true)?;
        }
        PtraceWaitPidEvent::Signaled { pid, signal: sig } => {
          let timestamp = Local::now();
          debug!("signaled: {pid}, {:?}", sig);
          let mut store = self.store.write().unwrap();
          if let Some(state) = store.get_current_mut(pid) {
            state.teriminate_otlp_span(timestamp);
            state.status = ProcessStatus::Exited(ProcessExit::Signal(sig));
            let associated_events = state.associated_events.clone();
            if !associated_events.is_empty() {
              self.msg_tx.send(
                ProcessStateUpdateEvent {
                  update: ProcessStateUpdate::Exit {
                    timestamp,
                    status: ProcessExit::Signal(sig),
                  },
                  pid,
                  ids: associated_events,
                }
                .into(),
              )?;
            }
            if pid == root_child {
              filterable_event!(TraceeExit {
                timestamp,
                signal: Some(sig),
                exit_code: 128 + sig.as_raw(),
              })
              .send_if_match(&self.msg_tx, self.filter)?;
              return Ok(ControlFlow::Break(()));
            }
          }
        }
        PtraceWaitPidEvent::Exited { pid, code } => {
          // pid could also be a not traced subprocess.
          let timestamp = Local::now();
          trace!("exited: pid {}, code {:?}", pid, code);
          let mut store = self.store.write().unwrap();
          if let Some(state) = store.get_current_mut(pid) {
            state.teriminate_otlp_span(timestamp);
            state.status = ProcessStatus::Exited(ProcessExit::Code(code));
            let associated_events = state.associated_events.clone();
            if !associated_events.is_empty() {
              self.msg_tx.send(
                ProcessStateUpdateEvent {
                  update: ProcessStateUpdate::Exit {
                    status: ProcessExit::Code(code),
                    timestamp,
                  },
                  pid,
                  ids: associated_events,
                }
                .into(),
              )?;
            }
            let should_exit = if pid == root_child {
              filterable_event!(TraceeExit {
                timestamp,
                signal: None,
                exit_code: code,
              })
              .send_if_match(&self.msg_tx, self.filter)?;
              true
            } else {
              false
            };
            if should_exit {
              return Ok(ControlFlow::Break(()));
            }
          }
        }
        PtraceWaitPidEvent::Continued(_) => unreachable!(),
        PtraceWaitPidEvent::StillAlive => break,
      }
      if counter > 10000 {
        // Give up if we have handled 100 events, so that we have a chance to handle other events
        debug!("yielding after 100 events");
        break;
      }
    }
    Ok(ControlFlow::Continue(()))
  }

  fn on_syscall_enter<'a>(
    &self,
    guard: Either<PtraceSyscallStopGuard<'a>, PtraceSeccompStopGuard<'a>>,
    pending_guards: &mut HashMap<Pid, PtraceStopGuard<'a>>,
  ) -> color_eyre::Result<()> {
    let timestamp = chrono::Local::now();
    let pid = guard.pid();
    let mut store = self.store.write().unwrap();
    let p = store.get_current_mut(pid).unwrap();
    p.presyscall = !p.presyscall;
    // SYSCALL ENTRY
    let info = match guard.syscall_info() {
      Ok(info) => info,
      Err(Errno::ESRCH) => {
        filterable_event!(Info(TracerEventMessage {
          timestamp: self.timestamp_now(),
          msg: "Failed to get syscall info: ESRCH (child probably gone!)".to_string(),
          pid: Some(pid),
        }))
        .send_if_match(&self.msg_tx, self.filter)?;
        info!("ptrace get_syscall_info failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
      }
      e => e?,
    };
    let regs = match guard.get_general_registers() {
      Ok(regs) => regs,
      Err(Errno::ESRCH) => {
        filterable_event!(Info(TracerEventMessage {
          timestamp: self.timestamp_now(),
          msg: "Failed to read registers: ESRCH (child probably gone!)".to_string(),
          pid: Some(pid),
        }))
        .send_if_match(&self.msg_tx, self.filter)?;
        info!("ptrace getregs failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
      }
      e => e?,
    };
    let syscallno = info.syscall_number().unwrap();
    let is_32bit = info.arch().is_32bit();
    // trace!("pre syscall: {syscallno}");
    if info.is_execveat().unwrap() {
      p.syscall = Syscall::Execveat;
      trace!("pre execveat {syscallno}");
      // int execveat(int dirfd, const char *pathname,
      //              char *const _Nullable argv[],
      //              char *const _Nullable envp[],
      //              int flags);
      let dirfd = regs.syscall_arg(0, is_32bit) as i32;
      let flags = regs.syscall_arg(4, is_32bit) as i32;
      let filename = match read_string(pid, regs.syscall_arg(1, is_32bit) as AddressType) {
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
      let argv = read_output_msg_array(pid, regs.syscall_arg(2, is_32bit) as AddressType, is_32bit);
      self.warn_for_argv(&argv, pid)?;
      let envp = read_env(pid, regs.syscall_arg(3, is_32bit) as AddressType, is_32bit);
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
        timestamp,
      ));
      p.timestamp = Some(timestamp);
    } else if info.is_execve().unwrap() {
      p.syscall = Syscall::Execve;
      trace!("pre execve {syscallno}",);
      let filename = read_arcstr(pid, regs.syscall_arg(0, is_32bit) as AddressType);
      let filename = self.get_filename_for_display(pid, filename)?;
      self.warn_for_filename(&filename, pid)?;
      let argv = read_output_msg_array(pid, regs.syscall_arg(1, is_32bit) as AddressType, is_32bit);
      self.warn_for_argv(&argv, pid)?;
      let envp = read_string_array(pid, regs.syscall_arg(2, is_32bit) as AddressType, is_32bit)
        .map(parse_envp);
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
        timestamp,
      ));
      p.timestamp = Some(timestamp);
    } else {
      p.syscall = Syscall::Other;
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
        pending_guards.insert(
          pid,
          match guard {
            Either::Left(l) => l.into(),
            Either::Right(r) => r.into(),
          },
        );
        return Ok(()); // Do not continue the syscall
      }
    }
    guard.cont_syscall(true)?;
    Ok(())
  }

  fn on_syscall_exit<'a>(
    &self,
    guard: PtraceSyscallStopGuard<'a>,
    pending_guards: &mut HashMap<Pid, PtraceStopGuard<'a>>,
  ) -> color_eyre::Result<()> {
    // SYSCALL EXIT
    // trace!("post syscall {}", p.syscall);
    let pid = guard.pid();
    let mut store = self.store.write().unwrap();
    let p = store.get_current_mut(pid).unwrap();
    p.presyscall = !p.presyscall;
    let result = match guard.syscall_info() {
      Ok(r) => r.syscall_result().unwrap(),
      Err(Errno::ESRCH) => {
        info!("ptrace get_syscall_info failed: {pid}, ESRCH, child probably gone!");
        return Ok(());
      }
      Err(e) => return Err(e.into()),
    };
    // If exec is successful, the register value might be clobbered.
    // TODO: would the value in ptrace_syscall_info be clobbered?
    let exec_result = if p.is_exec_successful { 0 } else { result } as i64;
    match p.syscall {
      Syscall::Execve | Syscall::Execveat => {
        trace!("post execve(at) in exec");
        if self.printer.args.successful_only && !p.is_exec_successful {
          p.exec_data = None;
          guard.seccomp_aware_cont_syscall(true)?;
          return Ok(());
        }
        if self.filter.intersects(TracerEventDetailsKind::Exec) {
          let id = TracerEvent::allocate_id();
          let (parent, parent_ctx) = p.parent_tracker.update_last_exec(id, exec_result == 0);
          let parent_ctx = parent_ctx.or_else(|| self.otlp.root_ctx());
          let exec = Self::collect_exec_event(
            &self.baseline.env,
            p,
            exec_result,
            p.exec_data.as_ref().unwrap().timestamp,
            parent,
          );
          if exec.result == 0 {
            let ctx = self
              .otlp
              .new_exec_ctx(&exec, parent_ctx.as_ref().map(|v| v.borrow()));
            if !self.otlp.span_could_end_at_exec() {
              if let Some(ctx) = ctx.clone() {
                p.otlp_ctxs.push(ctx);
              }
            }
            p.parent_tracker.update_last_exec_ctx(
              ctx,
              exec.timestamp,
              self.otlp.span_could_end_at_exec(),
            );
          } else {
            // TODO: generate an event on parent span
          }

          let event = TracerEventDetails::Exec(exec).into_event_with_id(id);
          p.associate_event([id]);
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
            pending_guards.insert(pid, guard.into());
            return Ok(()); // Do not continue the syscall
          }
        }

        p.exec_data = None;
        // update comm
        p.comm = read_comm(pid)?;
      }
      _ => (),
    }
    guard.seccomp_aware_cont_syscall(true)?;
    Ok(())
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
                timestamp: self.timestamp_now(),
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
              timestamp: self.timestamp_now(),
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
            timestamp: self.timestamp_now(),
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
            timestamp: self.timestamp_now(),
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
    timestamp: Timestamp,
    parent: Option<ParentEventId>,
  ) -> Box<ExecEvent> {
    let exec_data = state.exec_data.as_ref().unwrap();
    Box::new(ExecEvent {
      timestamp,
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
      parent,
    })
  }
}

static BREAKPOINT_ID: AtomicU32 = AtomicU32::new(0);

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
    pending_guards: &mut HashMap<Pid, PtraceStopGuard<'_>>,
  ) -> Result<(), Either<Errno, SendError<TracerMessage>>> {
    state.status = ProcessStatus::Running;
    let guard = pending_guards.remove(&state.pid).unwrap();
    if stop == BreakPointStop::SyscallEnter {
      guard.cont_syscall(false)
    } else {
      guard.seccomp_aware_cont_syscall(false)
    }
    .map_err(Either::Left)?;
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
    pending_guards: &mut HashMap<Pid, PtraceStopGuard<'_>>,
  ) -> Result<(), Errno> {
    state.pending_detach = Some(PendingDetach { signal, hid, hit });
    // Don't use *cont_with_signal because that causes
    // the loss of signal when we do it on syscall-enter.
    // Is this a kernel bug?
    if 0 != unsafe { libc::kill(state.pid.as_raw(), SENTINEL_SIGNAL.as_raw()) } {
      return Err(Errno::last());
    }
    let guard = pending_guards.remove(&state.pid).unwrap();
    if hit.stop == BreakPointStop::SyscallEnter {
      guard.cont_syscall(false)?;
    } else {
      guard.seccomp_aware_cont_syscall(false)?;
    }
    Ok(())
  }

  /// This function should only be called when in signal-delivery-stop if signal is not None. Otherwise, the signal might be ignored.
  fn detach_process_internal(
    &self,
    state: &mut ProcessState,
    signal: Option<(Signal, PtraceSignalDeliveryStopGuard<'_>)>,
    hid: u64,
    pending_guards: &mut HashMap<Pid, PtraceStopGuard<'_>>,
  ) -> Result<(), Either<Errno, SendError<TracerMessage>>> {
    let pid = state.pid;
    trace!("detaching: {pid}, signal: {:?}", signal);
    state.status = ProcessStatus::Detached;

    if let Some((sig, guard)) = signal {
      guard.injected_detach(sig)
    } else {
      let guard = pending_guards.remove(&state.pid).unwrap();
      guard.detach()
    }
    .inspect_err(|e| warn!("Failed to detach from {pid}: {e}"))
    .map_err(Either::Left)?;
    let timestamp = Local::now();
    trace!("detached: {pid}");
    let associated_events = state.associated_events.clone();
    self
      .msg_tx
      .send(
        ProcessStateUpdateEvent {
          update: ProcessStateUpdate::Detached { hid, timestamp },
          pid,
          ids: associated_events,
        }
        .into(),
      )
      .map_err(Either::Right)?;
    trace!("detach finished: {pid}");
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

  fn suspend_seccomp_bpf(&self, pid: Pid) -> Result<(), Errno> {
    use nix::libc::{PTRACE_O_SUSPEND_SECCOMP, PTRACE_SETOPTIONS, ptrace};

    if self.seccomp_bpf == SeccompBpf::On {
      unsafe {
        let result = ptrace(PTRACE_SETOPTIONS, pid, 0, PTRACE_O_SUSPEND_SECCOMP);
        if -1 == result {
          let errno = Errno::last();
          error!("Failed to suspend {pid}'s seccomp filter: {errno}");
          return Err(errno);
        } else {
          trace!("suspended {pid}'s seccomp filter successfully");
        }
      }
    }
    Ok(())
  }

  pub fn request_suspend_seccomp_bpf(&self, pid: Pid) -> color_eyre::Result<()> {
    trace!("received request to suspend {pid}'s seccomp-bpf filter");
    self.req_tx.send(PendingRequest::SuspendSeccompBpf(pid))?;
    Ok(())
  }

  pub fn seccomp_bpf(&self) -> bool {
    self.seccomp_bpf == SeccompBpf::On
  }
}

const SENTINEL_SIGNAL: Signal = Signal::Standard(nix::sys::signal::SIGSTOP);

impl Tracer {
  /// Returns current timestamp if timestamp is enabled
  fn timestamp_now(&self) -> Option<Timestamp> {
    if self.modifier_args.timestamp {
      Some(Local::now())
    } else {
      None
    }
  }
}
