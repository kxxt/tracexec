use std::{
  borrow::Cow,
  cell::RefCell,
  collections::{HashMap, HashSet},
  ffi::{CStr, CString},
  io::{self, stdin},
  iter::repeat,
  mem::MaybeUninit,
  os::{
    fd::{AsRawFd, RawFd},
    unix::{fs::MetadataExt, process::CommandExt},
  },
  process,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock, RwLock,
  },
  time::Duration,
};

use arcstr::ArcStr;
use color_eyre::{eyre::eyre, Section};
use enumflags2::{BitFlag, BitFlags};
use event::EventStorage;
use interface::BpfEventFlags;
use libbpf_rs::{
  num_possible_cpus,
  skel::{OpenSkel, Skel, SkelBuilder},
  ErrorKind, OpenObject, RingBuffer, RingBufferBuilder,
};
use nix::{
  errno::Errno,
  fcntl::OFlag,
  libc::{self, c_int, dup2, AT_FDCWD},
  sys::{
    signal::{kill, raise, Signal},
    wait::{waitpid, WaitPidFlag, WaitStatus},
  },
  unistd::{
    fork, getpid, initgroups, setpgid, setresgid, setresuid, setsid, tcsetpgrp, ForkResult, Gid,
    Pid, Uid, User,
  },
};
use process_tracker::ProcessTracker;
use skel::{
  types::{
    event_type, exec_event, exit_event, fd_event, fork_event, path_event, path_segment_event,
    tracexec_event_header,
  },
  TracexecSystemSkel,
};
use tokio::{
  sync::mpsc::{self, UnboundedSender},
  task::spawn_blocking,
};
use tracing::{debug, warn};

use crate::{
  cache::StringCache,
  cli::{
    args::{LogModeArgs, ModifierArgs},
    options::{Color, ExportFormat},
    Cli, EbpfCommand,
  },
  cmdbuilder::CommandBuilder,
  event::{
    filterable_event, ExecEvent, FilterableTracerEventDetails, FriendlyError, OutputMsg,
    ProcessStateUpdate, ProcessStateUpdateEvent, TracerEvent, TracerEventDetails,
    TracerEventDetailsKind, TracerMessage,
  },
  export::{self, JsonExecEvent, JsonMetaData},
  printer::{Printer, PrinterArgs, PrinterOut},
  proc::{cached_string, diff_env, parse_failiable_envp, BaselineInfo, FileDescriptorInfo},
  pty::{self, native_pty_system, PtySize, PtySystem},
  serialize_json_to_output,
  tracer::{
    state::{ExecData, ProcessExit},
    TracerMode,
  },
  tui::{self, app::App},
};

pub mod skel {
  include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/tracexec_system.skel.rs"
  ));
}

pub mod interface {
  include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bpf/interface.rs"));
}

mod event;
mod process_tracker;
pub use event::BpfError;

fn bump_memlock_rlimit() -> color_eyre::Result<()> {
  let rlimit = libc::rlimit {
    rlim_cur: 128 << 20,
    rlim_max: 128 << 20,
  };

  if unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlimit) } != 0 {
    return Err(
      color_eyre::eyre::eyre!("Failed to increase rlimit for memlock")
        .with_suggestion(|| "Try running as root"),
    );
  }

  Ok(())
}

pub struct EbpfTracer {
  cmd: Vec<String>,
  user: Option<User>,
  modifier: ModifierArgs,
  printer: Arc<Printer>,
  baseline: Arc<BaselineInfo>,
  tx: Option<UnboundedSender<TracerMessage>>,
  filter: BitFlags<TracerEventDetailsKind>,
  mode: TracerMode,
}

impl EbpfTracer {
  #[allow(clippy::needless_lifetimes)]
  pub fn spawn<'obj>(
    self,
    obj: &'obj mut MaybeUninit<OpenObject>,
    output: Option<Box<PrinterOut>>,
  ) -> color_eyre::Result<RunningEbpfTracer<'obj>> {
    let (skel, child) = self.spawn_command(obj)?;
    let follow_forks = !self.cmd.is_empty();
    let mut tracker = ProcessTracker::default();
    child.inspect(|p| tracker.add(*p));
    let mut builder = RingBufferBuilder::new();
    let event_storage: RefCell<HashMap<u64, EventStorage>> = RefCell::new(HashMap::new());
    let lost_events: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    let mut eid = 0;
    let printer_clone = self.printer.clone();
    let should_exit = Arc::new(AtomicBool::new(false));
    builder.add(&skel.maps.events, {
      let should_exit = should_exit.clone();
      move |data| {
        assert!(
          data.as_ptr() as usize % 8 == 0,
          "data is not 8 byte aligned!"
        );
        assert!(
          data.len() >= size_of::<tracexec_event_header>(),
          "data too short: {data:?}"
        );
        let header: &tracexec_event_header = unsafe { &*(data.as_ptr() as *const _) };
        match unsafe { header.r#type.assume_init() } {
          event_type::SYSENTER_EVENT => unreachable!(),
          event_type::SYSEXIT_EVENT => {
            #[allow(clippy::comparison_chain)]
            if header.eid > eid {
              // There are some lost events
              // In some cases the events are not really lost but sent out of order because of parallism
              warn!(
                "inconsistent event id counter: local = {eid}, kernel = {}. Possible event loss!",
                header.eid
              );
              lost_events.borrow_mut().extend(eid..header.eid);
              // reset local counter
              eid = header.eid + 1;
            } else if header.eid < eid {
              // This should only happen for lost events
              if lost_events.borrow_mut().remove(&header.eid) {
                // do nothing
                warn!("event {} is received out of order", header.eid);
              } else {
                panic!(
                  "inconsistent event id counter: local = {}, kernel = {}.",
                  eid, header.eid
                );
              }
            } else {
              // increase local counter for next event
              eid += 1;
            }
            assert_eq!(data.len(), size_of::<exec_event>());
            let event: exec_event = unsafe { std::ptr::read(data.as_ptr() as *const _) };
            if event.ret != 0 && self.modifier.successful_only {
              return 0;
            }
            let mut storage = event_storage.borrow_mut();
            let mut storage = storage.remove(&header.eid).unwrap();
            let envp = storage.strings.split_off(event.count[0] as usize);
            let argv = storage.strings;
            let cwd: OutputMsg = storage.paths.remove(&AT_FDCWD).unwrap().into();
            let eflags = BpfEventFlags::from_bits_truncate(header.flags);
            // TODO: How should we handle possible truncation?
            let base_filename = if eflags.contains(BpfEventFlags::FILENAME_READ_ERR) {
              OutputMsg::Err(FriendlyError::Bpf(BpfError::Flags))
            } else {
              cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.base_filename)).into()
            };
            let filename = if !unsafe { event.is_execveat.assume_init() } {
              base_filename
            } else {
              if base_filename.is_ok_and(|s| s.starts_with('/')) {
                base_filename
              } else {
                match event.fd {
                  AT_FDCWD => cwd.join(base_filename),
                  fd => {
                    // Check if it is a valid fd
                    if let Some(fdinfo) = storage.fdinfo_map.get(fd) {
                      fdinfo.path.clone().join(base_filename)
                    } else {
                      OutputMsg::PartialOk(cached_string(format!(
                        "[err: invalid fd: {fd}]/{base_filename}"
                      )))
                    }
                  }
                }
              }
            };
            let exec_data = ExecData::new(
              filename,
              Ok(argv),
              Ok(parse_failiable_envp(envp)),
              cwd,
              None,
              storage.fdinfo_map,
            );
            let pid = Pid::from_raw(header.pid);
            let comm = cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.comm));
            self
              .printer
              .print_exec_trace(
                pid,
                comm.clone(),
                event.ret,
                &exec_data,
                &self.baseline.env,
                &self.baseline.cwd,
              )
              .unwrap();
            if self.filter.intersects(TracerEventDetailsKind::Exec) {
              let event = TracerEvent::from(TracerEventDetails::Exec(Box::new(ExecEvent {
                pid,
                cwd: exec_data.cwd.clone(),
                comm,
                filename: exec_data.filename.clone(),
                argv: exec_data.argv.clone(),
                envp: exec_data.envp.clone(),
                interpreter: exec_data.interpreters.clone(),
                env_diff: exec_data
                  .envp
                  .as_ref()
                  .as_ref()
                  .map(|envp| diff_env(&self.baseline.env, envp))
                  .map_err(|e| *e),
                result: event.ret,
                fdinfo: exec_data.fdinfo.clone(),
              })));
              if follow_forks {
                tracker.associate_events(pid, [event.id])
              } else {
                tracker.force_associate_events(pid, [event.id])
              }
              self
                .tx
                .as_ref()
                .map(|tx| tx.send(event.into()))
                .transpose()
                .unwrap();
            }
          }
          event_type::STRING_EVENT => {
            let header_len = size_of::<tracexec_event_header>();
            let flags = BpfEventFlags::from_bits_truncate(header.flags);
            let msg = if flags.is_empty() {
              cached_cow(utf8_lossy_cow_from_bytes_with_nul(&data[header_len..])).into()
            } else {
              OutputMsg::Err(FriendlyError::Bpf(BpfError::Flags))
            };
            let mut storage = event_storage.borrow_mut();
            let strings = &mut storage.entry(header.eid).or_default().strings;
            // Catch event drop
            if strings.len() != header.id as usize {
              // Insert placeholders for dropped events
              let dropped_event = OutputMsg::Err(BpfError::Dropped.into());
              strings.extend(repeat(dropped_event).take(header.id as usize - strings.len()));
              debug_assert_eq!(strings.len(), header.id as usize);
            }
            // TODO: check flags in header
            strings.push(msg);
          }
          event_type::FD_EVENT => {
            assert_eq!(data.len(), size_of::<fd_event>());
            let event: &fd_event = unsafe { &*(data.as_ptr() as *const _) };
            let mut guard = event_storage.borrow_mut();
            let storage = guard.get_mut(&header.eid).unwrap();
            let fs = utf8_lossy_cow_from_bytes_with_nul(&event.fstype);
            let path = match fs.as_ref() {
              "pipefs" => OutputMsg::Ok(cached_string(format!("pipe:[{}]", event.ino))),
              "sockfs" => OutputMsg::Ok(cached_string(format!("socket:[{}]", event.ino))),
              "anon_inodefs" => OutputMsg::Ok(cached_string(format!(
                "anon_inode:{}",
                storage.paths.get(&event.path_id).unwrap().segments[0].as_ref()
              ))),
              _ => storage.paths.get(&event.path_id).unwrap().to_owned().into(),
            };
            let fdinfo = FileDescriptorInfo {
              fd: event.fd as RawFd,
              path,
              pos: event.pos as usize, // TODO: Handle error
              flags: OFlag::from_bits_retain(event.flags as c_int),
              mnt_id: event.mnt_id,
              ino: event.ino,
              mnt: cached_cow(fs),
              extra: vec![],
            };
            let fdc = &mut storage.fdinfo_map;
            fdc.fdinfo.insert(event.fd as RawFd, fdinfo);
          }
          event_type::PATH_EVENT => {
            assert_eq!(data.len(), size_of::<path_event>());
            let event: &path_event = unsafe { &*(data.as_ptr() as *const _) };
            let mut storage = event_storage.borrow_mut();
            let paths = &mut storage.entry(header.eid).or_default().paths;
            let path = paths.entry(header.id as i32).or_default();
            // FIXME
            path.is_absolute = true;
            assert_eq!(path.segments.len(), event.segment_count as usize);
            // eprintln!("Received path {} = {:?}", event.header.id, path);
          }
          event_type::PATH_SEGMENT_EVENT => {
            assert_eq!(data.len(), size_of::<path_segment_event>());
            let event: &path_segment_event = unsafe { &*(data.as_ptr() as *const _) };
            let mut storage = event_storage.borrow_mut();
            let paths = &mut storage.entry(header.eid).or_default().paths;
            let path = paths.entry(header.id as i32).or_default();
            // The segments must arrive in order.
            assert_eq!(path.segments.len(), event.index as usize);
            // TODO: check for errors
            path.segments.push(OutputMsg::Ok(cached_cow(
              utf8_lossy_cow_from_bytes_with_nul(&event.segment),
            )));
          }
          event_type::EXIT_EVENT => {
            assert_eq!(data.len(), size_of::<exit_event>());
            let event: &exit_event = unsafe { &*(data.as_ptr() as *const _) };
            debug!(
              "{} exited with code {}, signal {}",
              header.pid, event.code, event.sig
            );
            let pid = Pid::from_raw(header.pid);
            if let Some(associated) = tracker.maybe_associated_events(pid) {
              if !associated.is_empty() {
                self
                  .tx
                  .as_ref()
                  .map(|tx| {
                    tx.send(
                      ProcessStateUpdateEvent {
                        update: ProcessStateUpdate::Exit(match (event.sig, event.code) {
                          (0, code) => ProcessExit::Code(code),
                          (sig, _) => {
                            // 0x80 bit indicates coredump
                            ProcessExit::Signal(Signal::try_from(sig as i32 & 0x7f).unwrap())
                          }
                        }),
                        pid,
                        ids: associated.to_owned(),
                      }
                      .into(),
                    )
                  })
                  .transpose()
                  .unwrap();
              }
            }
            if unsafe { event.is_root_tracee.assume_init() } {
              self
                .tx
                .as_ref()
                .map(|tx| {
                  FilterableTracerEventDetails::from(match (event.sig, event.code) {
                    (0, exit_code) => TracerEventDetails::TraceeExit {
                      signal: None,
                      exit_code,
                    },
                    (sig, _) => {
                      // 0x80 bit indicates coredump
                      TracerEventDetails::TraceeExit {
                        signal: Some(Signal::try_from(sig as i32 & 0x7f).unwrap()),
                        exit_code: 128 + (sig as i32 & 0x7f),
                      }
                    }
                  })
                  .send_if_match(tx, self.filter)
                })
                .transpose()
                .unwrap();
              should_exit.store(true, Ordering::Relaxed);
            }
            if follow_forks {
              tracker.remove(pid);
            } else {
              tracker.maybe_remove(pid);
            }
          }
          event_type::FORK_EVENT => {
            assert_eq!(data.len(), size_of::<fork_event>());
            let event: &fork_event = unsafe { &*(data.as_ptr() as *const _) };
            // FORK_EVENT is only sent if follow_forks
            tracker.add(Pid::from_raw(header.pid));
            debug!("{} forked {}", event.parent_tgid, header.pid);
          }
        }
        0
      }
    })?;
    printer_clone.init_thread_local(output);
    let rb = builder.build()?;
    Ok(RunningEbpfTracer {
      rb,
      should_exit,
      skel,
    })
  }

  fn spawn_command<'obj>(
    &self,
    object: &'obj mut MaybeUninit<OpenObject>,
  ) -> color_eyre::Result<(TracexecSystemSkel<'obj>, Option<Pid>)> {
    let skel_builder = skel::TracexecSystemSkelBuilder::default();
    bump_memlock_rlimit()?;
    let mut open_skel = skel_builder.open(object)?;
    let ncpu = num_possible_cpus()?.try_into().expect("Too many cores!");
    open_skel.maps.rodata_data.tracexec_config.max_num_cpus = ncpu;
    open_skel.maps.cache.set_max_entries(ncpu)?;
    // tracexec runs in the same pid namespace with the tracee
    let pid_ns_ino = std::fs::metadata("/proc/self/ns/pid")?.ino();
    let (skel, child) = if !self.cmd.is_empty() {
      let mut cmd = CommandBuilder::new(&self.cmd[0]);
      cmd.args(self.cmd.iter().skip(1));
      cmd.cwd(std::env::current_dir()?);
      let mut cmd = cmd.as_command()?;
      match unsafe { fork()? } {
        ForkResult::Parent { child } => {
          self
            .tx
            .as_ref()
            .map(|tx| filterable_event!(TraceeSpawn(child)).send_if_match(tx, self.filter))
            .transpose()?;
          if matches!(&self.mode, TracerMode::Log { foreground: true }) {
            match tcsetpgrp(stdin(), child) {
              Ok(_) => {}
              Err(Errno::ENOTTY) => {
                eprintln!("tcsetpgrp failed: ENOTTY");
              }
              r => r?,
            }
          }
          open_skel.maps.rodata_data.tracexec_config.follow_fork = MaybeUninit::new(true);
          open_skel.maps.rodata_data.tracexec_config.tracee_pid = child.as_raw();
          open_skel.maps.rodata_data.tracexec_config.tracee_pidns_inum = pid_ns_ino as u32;
          let mut skel = open_skel.load()?;
          skel.attach()?;
          match waitpid(child, Some(WaitPidFlag::WSTOPPED))? {
            terminated @ WaitStatus::Exited(_, _) | terminated @ WaitStatus::Signaled(_, _, _) => {
              panic!("Child exited abnormally before tracing is started: status: {terminated:?}");
            }
            WaitStatus::Stopped(_, _) => kill(child, Signal::SIGCONT)?,
            _ => unreachable!("Invalid wait status!"),
          }
          (skel, Some(child))
        }
        ForkResult::Child => {
          let slave_pty = match &self.mode {
            TracerMode::Tui(tty) => tty.as_ref(),
            _ => None,
          };

          if let Some(pts) = slave_pty {
            unsafe {
              dup2(pts.fd.as_raw_fd(), 0);
              dup2(pts.fd.as_raw_fd(), 1);
              dup2(pts.fd.as_raw_fd(), 2);
            }
            setsid()?;
            if unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) } == -1 {
              Err(io::Error::last_os_error())?;
            }
          } else if matches!(self.mode, TracerMode::Tui(_)) {
            unsafe {
              let dev_null = std::fs::File::open("/dev/null")?;
              dup2(dev_null.as_raw_fd(), 0);
              dup2(dev_null.as_raw_fd(), 1);
              dup2(dev_null.as_raw_fd(), 2);
            }
          } else {
            let me = getpid();
            setpgid(me, me)?;
          }

          // Wait for eBPF program to load
          raise(Signal::SIGSTOP)?;

          if let Some(user) = &self.user {
            initgroups(&CString::new(user.name.as_str())?[..], user.gid)?;
            setresgid(user.gid, user.gid, Gid::from_raw(u32::MAX))?;
            setresuid(user.uid, user.uid, Uid::from_raw(u32::MAX))?;
          }

          // Clean up a few things before we exec the program
          // Clear out any potentially problematic signal
          // dispositions that we might have inherited
          for signo in &[
            libc::SIGCHLD,
            libc::SIGHUP,
            libc::SIGINT,
            libc::SIGQUIT,
            libc::SIGTERM,
            libc::SIGALRM,
          ] {
            unsafe {
              libc::signal(*signo, libc::SIG_DFL);
            }
          }
          unsafe {
            let empty_set: libc::sigset_t = std::mem::zeroed();
            libc::sigprocmask(libc::SIG_SETMASK, &empty_set, std::ptr::null_mut());
          }

          pty::close_random_fds();

          return Err(cmd.exec().into());
        }
      }
    } else {
      let mut skel = open_skel.load()?;
      skel.attach()?;
      (skel, None)
    };
    Ok((skel, child))
  }
}

// TODO: we should start polling the ringbuffer before program load

pub struct RunningEbpfTracer<'obj> {
  rb: RingBuffer<'obj>,
  should_exit: Arc<AtomicBool>,
  // The eBPF program gets unloaded on skel drop
  #[allow(unused)]
  skel: TracexecSystemSkel<'obj>,
}

impl<'rb> RunningEbpfTracer<'rb> {
  pub fn run_until_exit(&self) {
    loop {
      if self.should_exit.load(Ordering::Relaxed) {
        break;
      }
      match self.rb.poll(Duration::from_millis(100)) {
        Ok(_) => continue,
        Err(e) => {
          if e.kind() == ErrorKind::Interrupted {
            continue;
          } else {
            panic!("Failed to poll ringbuf: {e}");
          }
        }
      }
    }
  }
}

pub async fn run(
  command: EbpfCommand,
  user: Option<User>,
  color: Color,
) -> color_eyre::Result<()> {
  let obj = Box::leak(Box::new(MaybeUninit::uninit()));
  match command {
    EbpfCommand::Log {
      cmd,
      output,
      modifier_args,
      log_args,
    } => {
      let modifier_args = modifier_args.processed();
      let baseline = Arc::new(BaselineInfo::new()?);
      let output = Cli::get_output(output, color)?;
      let printer = Arc::new(Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      ));
      let tracer = EbpfTracer {
        cmd,
        user,
        modifier: modifier_args,
        printer,
        baseline,
        tx: None,
        filter: TracerEventDetailsKind::empty(), // FIXME
        mode: TracerMode::Log {
          foreground: log_args.foreground(),
        },
      };
      let running_tracer = tracer.spawn(obj, Some(output))?;
      running_tracer.run_until_exit();
      Ok(())
    }
    EbpfCommand::Tui {
      cmd,
      modifier_args,
      tracer_event_args,
      tui_args,
    } => {
      let follow_forks = !cmd.is_empty();
      if tui_args.tty && !follow_forks {
        return Err(
          eyre!("--tty is not supported for eBPF system-wide tracing.").with_suggestion(|| {
            "Did you mean to use follow-fork mode? e.g. tracexec ebpf tui -t -- bash"
          }),
        );
      }
      let modifier_args = modifier_args.processed();
      // Disable owo-colors when running TUI
      owo_colors::control::set_should_colorize(false);
      let (baseline, tracer_mode, pty_master) = if tui_args.tty {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
          rows: 24,
          cols: 80,
          pixel_width: 0,
          pixel_height: 0,
        })?;
        (
          BaselineInfo::with_pts(&pair.slave)?,
          TracerMode::Tui(Some(pair.slave)),
          Some(pair.master),
        )
      } else {
        (BaselineInfo::new()?, TracerMode::Tui(None), None)
      };
      let baseline = Arc::new(baseline);
      let frame_rate = tui_args.frame_rate.unwrap_or(60.);
      let log_args = LogModeArgs {
        show_cmdline: false, // We handle cmdline in TUI
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        diff_env: true,
        ..Default::default()
      };
      let mut app = App::new(
        None,
        &log_args,
        &modifier_args,
        tui_args,
        baseline.clone(),
        pty_master,
      )?;
      app.activate_experiment("eBPF");
      let printer = Arc::new(Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      ));
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      // let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = EbpfTracer {
        cmd,
        user,
        modifier: modifier_args,
        printer,
        baseline,
        filter: tracer_event_args.filter()?,
        tx: Some(tracer_tx),
        mode: tracer_mode,
      };
      let running_tracer = tracer.spawn(obj, None)?;
      let should_exit = running_tracer.should_exit.clone();
      let tracer_thread = spawn_blocking(move || {
        running_tracer.run_until_exit();
      });
      let mut tui = tui::Tui::new()?.frame_rate(frame_rate);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit()?;
      tui::restore_tui()?;
      if !follow_forks {
        should_exit.store(true, Ordering::Relaxed);
      }
      tracer_thread.await?;
      Ok(())
    }
    EbpfCommand::Collect {
      cmd,
      modifier_args,
      format,
      pretty,
      output,
      foreground,
      no_foreground,
    } => {
      let modifier_args = modifier_args.processed();
      let baseline = Arc::new(BaselineInfo::new()?);
      let mut output = Cli::get_output(output, color)?;
      let log_args = LogModeArgs {
        show_cmdline: false,
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        diff_env: false,
        foreground,
        no_foreground,
        ..Default::default()
      };
      let printer = Arc::new(Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      ));
      let (tx, mut rx) = mpsc::unbounded_channel();
      let tracer = EbpfTracer {
        cmd,
        user,
        modifier: modifier_args,
        printer,
        baseline: baseline.clone(),
        tx: Some(tx),
        filter: TracerEventDetailsKind::all(),
        mode: TracerMode::Log {
          foreground: log_args.foreground(),
        },
      };
      let running_tracer = tracer.spawn(obj, None)?;
      let tracer_thread = spawn_blocking(move || {
        running_tracer.run_until_exit();
      });
      match format {
        ExportFormat::Json => {
          let mut json = export::Json {
            meta: JsonMetaData::new(baseline.as_ref().to_owned()),
            events: Vec::new(),
          };
          loop {
            match rx.recv().await {
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::TraceeExit { exit_code, .. },
                ..
              })) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                serialize_json_to_output(&mut output, &json, pretty)?;
                output.write_all(b"\n")?;
                output.flush()?;
                process::exit(exit_code);
              }
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::Exec(exec),
                id,
              })) => {
                json.events.push(JsonExecEvent::new(id, *exec));
              }
              // channel closed abnormally.
              None | Some(TracerMessage::FatalError(_)) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                process::exit(1);
              }
              _ => (),
            }
          }
        }
        ExportFormat::JsonStream => {
          serialize_json_to_output(
            &mut output,
            &JsonMetaData::new(baseline.as_ref().to_owned()),
            pretty,
          )?;
          loop {
            match rx.recv().await {
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::TraceeExit { exit_code, .. },
                ..
              })) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                process::exit(exit_code);
              }
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::Exec(exec),
                id,
              })) => {
                let json_event = JsonExecEvent::new(id, *exec);
                serialize_json_to_output(&mut output, &json_event, pretty)?;
                output.write_all(b"\n")?;
                output.flush()?;
              }
              // channel closed abnormally.
              None | Some(TracerMessage::FatalError(_)) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                process::exit(1);
              }
              _ => (),
            }
          }
        }
      }
    }
  }
}

fn utf8_lossy_cow_from_bytes_with_nul(data: &[u8]) -> Cow<str> {
  String::from_utf8_lossy(CStr::from_bytes_until_nul(data).unwrap().to_bytes())
}

fn cached_cow(cow: Cow<str>) -> ArcStr {
  let cache = CACHE.get_or_init(|| Arc::new(RwLock::new(StringCache::new())));
  match cow {
    Cow::Borrowed(s) => cache.write().unwrap().get_or_insert(s),
    Cow::Owned(s) => cache.write().unwrap().get_or_insert_owned(s),
  }
}

static CACHE: OnceLock<Arc<RwLock<StringCache>>> = OnceLock::new();
