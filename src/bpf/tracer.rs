use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  ffi::OsStr,
  io::stdin,
  iter::repeat_n,
  mem::MaybeUninit,
  os::{fd::RawFd, unix::fs::MetadataExt},
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use super::interface::BpfEventFlags;
use super::process_tracker::ProcessTracker;
use super::skel::{
  TracexecSystemSkel,
  types::{
    event_type, exec_event, exit_event, fd_event, fork_event, path_event, path_segment_event,
    tracexec_event_header,
  },
};
use super::{event::EventStorage, skel::TracexecSystemSkelBuilder};
use crate::{
  bpf::{BpfError, cached_cow, utf8_lossy_cow_from_bytes_with_nul},
  timestamp::ts_from_boot_ns,
  tracee,
  tracer::TracerBuilder,
};
use chrono::Local;
use color_eyre::Section;
use enumflags2::{BitFlag, BitFlags};
use libbpf_rs::{
  ErrorKind, OpenObject, RingBuffer, RingBufferBuilder, num_possible_cpus,
  skel::{OpenSkel, Skel, SkelBuilder},
};
use nix::{
  errno::Errno,
  fcntl::OFlag,
  libc::{self, AT_FDCWD, c_int},
  sys::{
    signal::{kill, raise},
    wait::{WaitPidFlag, WaitStatus, waitpid},
  },
  unistd::{Pid, User, tcsetpgrp},
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, warn};

use crate::{
  cli::args::ModifierArgs,
  cmdbuilder::CommandBuilder,
  event::{
    ExecEvent, FilterableTracerEventDetails, FriendlyError, OutputMsg, ProcessStateUpdate,
    ProcessStateUpdateEvent, TracerEvent, TracerEventDetails, TracerEventDetailsKind,
    TracerMessage, filterable_event,
  },
  printer::{Printer, PrinterOut},
  proc::{BaselineInfo, FileDescriptorInfo, cached_string, diff_env, parse_failiable_envp},
  ptrace::Signal,
  pty::{self},
  tracer::{ExecData, ProcessExit, TracerMode},
};

pub struct EbpfTracer {
  user: Option<User>,
  modifier: ModifierArgs,
  printer: Arc<Printer>,
  baseline: Arc<BaselineInfo>,
  tx: Option<UnboundedSender<TracerMessage>>,
  filter: BitFlags<TracerEventDetailsKind>,
  mode: TracerMode,
}

impl TracerBuilder {
  /// Build a [`EbpfTracer`].
  ///
  /// Panics on unset required fields.
  pub fn build_ebpf(self) -> EbpfTracer {
    EbpfTracer {
      user: self.user,
      modifier: self.modifier,
      printer: Arc::new(self.printer.unwrap()),
      baseline: self.baseline.unwrap(),
      tx: self.tx,
      filter: self
        .filter
        .unwrap_or_else(BitFlags::<TracerEventDetailsKind>::all),
      mode: self.mode.unwrap(),
    }
  }
}

impl EbpfTracer {
  #[allow(clippy::needless_lifetimes)]
  pub fn spawn<'obj>(
    self,
    cmd: &[impl AsRef<OsStr>],
    obj: &'obj mut MaybeUninit<OpenObject>,
    output: Option<Box<PrinterOut>>,
  ) -> color_eyre::Result<RunningEbpfTracer<'obj>> {
    let (skel, child) = self.spawn_command(obj, cmd)?;
    let follow_forks = !cmd.is_empty();
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
              // In some cases the events are not really lost but sent out of order because of parallelism
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
            let filename = if !unsafe { event.is_execveat.assume_init() }
              || base_filename.is_ok_and(|s| s.starts_with('/'))
            {
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
            };
            let exec_data = ExecData::new(
              filename,
              Ok(argv),
              Ok(parse_failiable_envp(envp)),
              cwd,
              None,
              storage.fdinfo_map,
              ts_from_boot_ns(event.timestamp),
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
                timestamp: exec_data.timestamp,
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
              strings.extend(repeat_n(dropped_event, header.id as usize - strings.len()));
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
                        update: ProcessStateUpdate::Exit {
                          timestamp: ts_from_boot_ns(event.timestamp),
                          status: match (event.sig, event.code) {
                            (0, code) => ProcessExit::Code(code),
                            (sig, _) => {
                              // 0x80 bit indicates coredump
                              ProcessExit::Signal(Signal::from_raw(sig as i32 & 0x7f))
                            }
                          },
                        },
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
                      timestamp: ts_from_boot_ns(event.timestamp),
                      signal: None,
                      exit_code,
                    },
                    (sig, _) => {
                      // 0x80 bit indicates coredump
                      TracerEventDetails::TraceeExit {
                        timestamp: ts_from_boot_ns(event.timestamp),
                        signal: Some(Signal::from_raw(sig as i32 & 0x7f)),
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
    cmd: &[impl AsRef<OsStr>],
  ) -> color_eyre::Result<(TracexecSystemSkel<'obj>, Option<Pid>)> {
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

    let skel_builder = TracexecSystemSkelBuilder::default();
    bump_memlock_rlimit()?;
    let mut open_skel = skel_builder.open(object)?;
    let ncpu = num_possible_cpus()?.try_into().expect("Too many cores!");
    open_skel.maps.rodata_data.tracexec_config.max_num_cpus = ncpu;
    open_skel.maps.cache.set_max_entries(ncpu)?;
    // tracexec runs in the same pid namespace with the tracee
    let pid_ns_ino = std::fs::metadata("/proc/self/ns/pid")?.ino();
    let (skel, child) = if !cmd.is_empty() {
      let mut cmdbuilder = CommandBuilder::new(cmd[0].as_ref());
      cmdbuilder.args(cmd.iter().skip(1));
      cmdbuilder.cwd(std::env::current_dir()?);
      let pts = match &self.mode {
        TracerMode::Tui(tty) => tty.as_ref(),
        _ => None,
      };
      let with_tty = match &self.mode {
        TracerMode::Tui(tty) => tty.is_some(),
        TracerMode::Log { .. } => true,
      };
      let use_pseudo_term = pts.is_some();
      let user = self.user.clone();
      let child = pty::spawn_command(pts, cmdbuilder, move |_program_path| {
        if !with_tty {
          tracee::nullify_stdio()?;
        }

        if use_pseudo_term {
          tracee::lead_session_and_control_terminal()?;
        } else {
          tracee::lead_process_group()?;
        }

        // Wait for eBPF program to load
        raise(nix::sys::signal::SIGSTOP)?;

        if let Some(user) = &user {
          tracee::runas(user, None)?;
        }

        Ok(())
      })?;

      self
        .tx
        .as_ref()
        .map(|tx| {
          filterable_event!(TraceeSpawn {
            pid: child,
            timestamp: Local::now()
          })
          .send_if_match(tx, self.filter)
        })
        .transpose()?;
      if matches!(&self.mode, TracerMode::Log { foreground: true }) {
        match tcsetpgrp(stdin(), child) {
          Ok(_) => {}
          Err(Errno::ENOTTY) => {
            warn!("tcsetpgrp failed: ENOTTY");
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
        WaitStatus::Stopped(_, _) => kill(child, nix::sys::signal::SIGCONT)?,
        _ => unreachable!("Invalid wait status!"),
      }
      (skel, Some(child))
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
  pub should_exit: Arc<AtomicBool>,
  // The eBPF program gets unloaded on skel drop
  #[allow(unused)]
  skel: TracexecSystemSkel<'obj>,
}

impl RunningEbpfTracer<'_> {
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
