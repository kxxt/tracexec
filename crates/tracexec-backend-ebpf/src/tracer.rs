use std::{
  cell::RefCell,
  collections::{
    HashMap,
    HashSet,
  },
  ffi::OsStr,
  io::stdin,
  iter::repeat_n,
  mem::MaybeUninit,
  os::{
    fd::RawFd,
    unix::fs::MetadataExt,
  },
  sync::{
    Arc,
    atomic::{
      AtomicBool,
      Ordering,
    },
  },
  time::Duration,
};

use chrono::Local;
use color_eyre::Section;
use enumflags2::{
  BitFlag,
  BitFlags,
};
use libbpf_rs::{
  OpenObject,
  RingBuffer,
  RingBufferBuilder,
  num_possible_cpus,
  skel::{
    OpenSkel,
    Skel,
    SkelBuilder,
  },
};
use nix::{
  errno::Errno,
  fcntl::OFlag,
  libc::{
    self,
    AT_FDCWD,
    c_int,
  },
  sys::{
    signal::{
      kill,
      raise,
    },
    wait::{
      WaitPidFlag,
      WaitStatus,
      waitpid,
    },
  },
  unistd::{
    Pid,
    User,
    tcsetpgrp,
  },
};
use tokio::sync::mpsc::UnboundedSender;
use tracexec_core::{
  cli::args::ModifierArgs,
  cmdbuilder::CommandBuilder,
  event::{
    ExecEvent,
    ExecSyscall,
    FilterableTracerEventDetails,
    OutputMsg,
    ProcessStateUpdate,
    ProcessStateUpdateEvent,
    TracerEvent,
    TracerEventDetails,
    TracerEventDetailsKind,
    TracerMessage,
    filterable_event,
  },
  printer::{
    Printer,
    PrinterOut,
  },
  proc::{
    BaselineInfo,
    CgroupInfo,
    FileDescriptorInfo,
    diff_env,
  },
  pty,
  timestamp::ts_from_boot_ns,
  tracee,
  tracer::{
    ExecData,
    ProcessExit,
    Signal,
    TracerBuilder,
    TracerMode,
  },
};
use tracing::{
  debug,
  warn,
};

use self::private::Sealed;
use crate::{
  bpf::{
    BpfError,
    cached_cow,
    interface::BpfEventFlags,
    skel::{
      TracexecSystemSkel,
      TracexecSystemSkelBuilder,
      types::{
        event_type,
        exec_event,
        exit_event,
        fd_event,
        fork_event,
        path_event,
        path_segment_event,
        tracexec_event_header,
      },
    },
    utf8_lossy_cow_from_bytes_with_nul,
  },
  cgroup_cache::CgroupCache,
  event::EventStorage,
  parser::{
    parse_groups_event,
    parse_path_segment,
    parse_string_event,
    process_argv,
    process_base_filename,
    process_cred,
    process_envp,
    process_filename,
    process_path,
  },
  probe::{
    kernel_have_ftrace_with_direct_calls,
    kernel_have_syscall_wrappers,
  },
  process_tracker::ProcessTracker,
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

mod private {
  use tracexec_core::tracer::TracerBuilder;

  pub trait Sealed {}

  impl Sealed for TracerBuilder {}
}

pub trait BuildEbpfTracer: Sealed {
  fn build_ebpf(self) -> EbpfTracer;
}

impl BuildEbpfTracer for TracerBuilder {
  /// Build a [`EbpfTracer`].
  ///
  /// Panics on unset required fields.
  fn build_ebpf(self) -> EbpfTracer {
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
    let cgroup_cache = if self.modifier.collect_cgroup {
      Some(CgroupCache::new())
    } else {
      None
    };
    builder.add(&skel.maps.events, {
      let should_exit = should_exit.clone();
      move |data| {
        assert!(
          (data.as_ptr() as usize).is_multiple_of(8),
          "data is not 8 byte aligned!"
        );
        assert!(
          data.len() >= size_of::<tracexec_event_header>(),
          "data too short: {data:?}"
        );
        let header: &tracexec_event_header = unsafe { &*(data.as_ptr() as *const _) };
        match header.r#type {
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
            let event: &exec_event = unsafe { &*(data.as_ptr() as *const _) };
            let eflags = BpfEventFlags::from_bits_truncate(header.flags);
            let mut storage = event_storage.borrow_mut();
            let mut storage = storage.remove(&header.eid).unwrap();
            if event.ret != 0 && self.modifier.successful_only {
              return 0;
            }
            let mut has_dash_env = false;
            let envp = process_envp(
              eflags,
              storage.strings.split_off(event.count[0] as usize),
              &mut has_dash_env,
            );
            let argv = process_argv(eflags, storage.strings);
            let cwd: OutputMsg = storage.paths.remove(&AT_FDCWD).unwrap().into();
            // TODO: How should we handle possible truncation?
            let base_filename = process_base_filename(eflags, event);
            let filename = process_filename(base_filename, event, &cwd, &storage.fdinfo_map);
            let cred = process_cred(eflags, event, storage.groups);
            let cgroup = match &cgroup_cache {
              Some(cache) => cache.resolve(event.cgroup_id),
              None => CgroupInfo::NotCollected,
            };
            let exec_data = ExecData::new(
              Pid::from_raw(header.pid),
              filename,
              argv,
              envp,
              has_dash_env,
              cred,
              cwd,
              None,
              storage.fdinfo_map,
              ts_from_boot_ns(event.timestamp),
              cgroup,
            );
            // Pid of the thread that triggers execve syscall
            // let pid = Pid::from_raw(header.pid);
            let tgid = Pid::from_raw(event.tgid);
            let comm = cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.comm));
            self
              .printer
              .print_exec_trace(
                tgid,
                comm.clone(),
                event.ret,
                &exec_data,
                &self.baseline.env,
                &self.baseline.cwd,
              )
              .unwrap();
            if self.filter.intersects(TracerEventDetailsKind::Exec) {
              let is_execveat = unsafe { event.is_execveat.assume_init() };
              let syscall = if is_execveat {
                ExecSyscall::Execveat
              } else {
                ExecSyscall::Execve
              };
              let id = TracerEvent::allocate_id();
              debug!(
                "Looking up parent tracker for {} (follow_forks={follow_forks})",
                tgid
              );
              // When a non-main thread calls exec, the kernel will destroy all
              // threads and replace it with a new process (using it's old tgid).
              //
              // When that happens, sched_process_exit happens before sched_process_exec
              // and the parent tracker for the process is cleaned-up.
              //
              // Thus we need to allocate a new parent tracker for it in this case.
              // Unfortunately I cannot find a way to tell this from normal exit in bpf.
              // Using sched_process_free also proved to be unreliable.
              //
              // Example:
              //
              //             bash-45965   [005] ...11  6665.239304: tracexec_system: 1 bash execve target/debug/exec-in-thread UID: 1000 GID: 1000 PID: 45965
              //
              //             bash-45965   [005] ...11  6665.243477: tracexec_system: Reading pwd...
              //             bash-45965   [005] ...21  6665.243673: tracexec_system: sched_process_exec: pid=45965, tgid=45965
              //             bash-45965   [005] ...11  6665.243696: tracexec_system: execve result: 0 PID 45965
              //
              //             bash-45965   [005] ...11  6665.243749: tracexec_system: Ringbuf stat: avail: 40, cons: 46640, prod: 46680
              //   exec-in-thread-45966   [002] ...11  6665.244308: tracexec_system: 2 exec-in-thread execve /home/player/repos/tracexec-trees/agent01/target/debug/exec-in-thread UID: 1000 GID: 1000 PID: 45966
              //
              //   exec-in-thread-45966   [002] ...11  6665.244743: tracexec_system: Reading pwd...
              //   exec-in-thread-45965   [005] ...21  6665.244817: tracexec_system: sched_process_exit: pid=45965, tgid=45965
              //   exec-in-thread-45965   [002] ...21  6665.245014: tracexec_system: sched_process_exec: pid=45965, tgid=45965
              //   exec-in-thread-45965   [002] ...11  6665.245024: tracexec_system: execve result: 0 PID 45965
              //
              //   exec-in-thread-45965   [002] ...11  6665.245084: tracexec_system: Ringbuf stat: avail: 40, cons: 63248, prod: 63288
              //   exec-in-thread-45965   [002] ...21  6665.245445: tracexec_system: sched_process_exit: pid=45965, tgid=45965
              //            <...>-45967   [005] ...21  6665.246146: tracexec_system: sched_process_exit: pid=45967, tgid=45967
              //            <...>-45969   [005] ...21  6665.247391: tracexec_system: sched_process_exit: pid=45969, tgid=45969
              //
              if !tracker.contains(tgid) {
                tracker.add(tgid);
              }
              let parent_tracker = tracker.parent_tracker_mut(tgid).unwrap();
              let parent = parent_tracker.update_last_exec(id, event.ret == 0);
              let event = TracerEventDetails::Exec(Box::new(ExecEvent {
                syscall,
                // header.pid is the thread id that entered execve/execveat.
                exec_pid: Pid::from_raw(header.pid),
                timestamp: exec_data.timestamp,
                pid: tgid,
                cwd: exec_data.cwd.clone(),
                comm,
                filename: exec_data.filename.clone(),
                argv: exec_data.argv.clone(),
                envp: exec_data.envp.clone(),
                has_dash_env,
                cred: exec_data.cred,
                interpreter: exec_data.interpreters.clone(),
                env_diff: exec_data
                  .envp
                  .as_ref()
                  .as_ref()
                  .map(|envp| diff_env(&self.baseline.env, envp))
                  .map_err(|e| *e),
                result: event.ret,
                fdinfo: exec_data.fdinfo.clone(),
                parent,
                cgroup: exec_data.cgroup.clone(),
              }))
              .into_event_with_id(id);
              if follow_forks {
                tracker.associate_events(tgid, [event.id])
              } else {
                tracker.force_associate_events(tgid, [event.id])
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
            let msg = parse_string_event(header, data);
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
            let path = process_path(event, &fs, &storage.paths);
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
            path.segments.push(parse_path_segment(data));
          }
          event_type::GROUPS_EVENT => {
            let mut storage = event_storage.borrow_mut();
            let groups_result = &mut storage.entry(header.eid).or_default().groups;
            assert!(groups_result.is_err());
            *groups_result = Ok(parse_groups_event(data));
          }
          event_type::EXIT_EVENT => {
            assert_eq!(data.len(), size_of::<exit_event>());
            let event: &exit_event = unsafe { &*(data.as_ptr() as *const _) };
            debug!(
              "{} exited with code {}, signal {}",
              header.pid, event.code, event.sig
            );
            let pid = Pid::from_raw(header.pid);
            if let Some(associated) = tracker.maybe_associated_events(pid)
              && !associated.is_empty()
            {
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
            let fork_parent = Pid::from_raw(event.parent_tgid);
            let pid = Pid::from_raw(header.pid);
            debug!(
              "Allocating parent tracker for {} (follow_forks={follow_forks})",
              pid
            );
            tracker.add(pid);
            if let [Some(curr), Some(par)] = tracker.parent_tracker_disjoint_mut(pid, fork_parent) {
              // Parent can be missing if the fork happens before tracexec start.
              curr.save_parent_last_exec(par);
            }
            debug!("{} forked {}", event.parent_tgid, header.pid);
          }
          _ => {
            unreachable!()
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
    let ncpu: u32 = num_possible_cpus()?.try_into().expect("Too many cores!");
    let rodata = open_skel.maps.rodata_data.as_deref_mut().unwrap();
    let kconfig = procfs::kernel_config()
      .inspect_err(|e| warn!("Failed to get kernel config: {e}"))
      .ok();
    // Check if we should use kprobe on kernels without CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS
    if !kernel_have_ftrace_with_direct_calls(kconfig.as_ref()) {
      open_skel.progs.sys_execve_fentry.set_autoload(false);
      open_skel.progs.sys_execveat_fentry.set_autoload(false);
      open_skel.progs.sys_exit_execve_fexit.set_autoload(false);
      open_skel.progs.sys_exit_execveat_fexit.set_autoload(false);
    } else {
      open_skel.progs.sys_execve_kprobe.set_autoload(false);
      open_skel.progs.sys_execveat_kprobe.set_autoload(false);
      open_skel
        .progs
        .sys_exit_execve_kretprobe
        .set_autoload(false);
      open_skel
        .progs
        .sys_exit_execveat_kretprobe
        .set_autoload(false);
    }

    let kernel_have_syscall_wrappers = kernel_have_syscall_wrappers(kconfig.as_ref());
    if !kernel_have_syscall_wrappers {
      // Only handle kprobe here because the only supported kernels
      // that could trigger it is riscv linux < 6.6, which won't
      // support ftrace_with_direct_calls anyway.
      open_skel.progs.sys_execve_kprobe.set_autoattach(false);
      open_skel.progs.sys_execveat_kprobe.set_autoattach(false);
      open_skel
        .progs
        .sys_exit_execve_kretprobe
        .set_autoattach(false);
      open_skel
        .progs
        .sys_exit_execveat_kretprobe
        .set_autoattach(false);
    }

    // Considering that bpf is preemept-able, we must be prepared to handle more entries.
    let cache_size = 2 * ncpu;
    open_skel.maps.cache.set_max_entries(cache_size)?;
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
      rodata.tracexec_config.follow_fork = MaybeUninit::new(true);
      rodata.tracexec_config.tracee_pid = child.as_raw();
      rodata.tracexec_config.tracee_pidns_inum = pid_ns_ino as u32;

      let mut skel = open_skel.load()?;
      skel.attach()?;
      if !kernel_have_syscall_wrappers {
        attach_kprobes_without_syscall_wrappers(&mut skel)?;
      }

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
      if !kernel_have_syscall_wrappers {
        attach_kprobes_without_syscall_wrappers(&mut skel)?;
      }
      (skel, None)
    };
    Ok((skel, child))
  }
}

pub struct RunningEbpfTracer<'obj> {
  rb: RingBuffer<'obj>,
  pub should_exit: Arc<AtomicBool>,
  // The eBPF program gets unloaded on skel drop
  #[allow(unused)]
  skel: TracexecSystemSkel<'obj>,
}

impl RunningEbpfTracer<'_> {
  pub fn run_until_exit(&self) {
    run_until_exit_impl(&self.should_exit, || {
      self.rb.poll(Duration::from_millis(100))
    })
  }
}

fn run_until_exit_impl<F>(should_exit: &AtomicBool, mut poll: F)
where
  F: FnMut() -> Result<(), libbpf_rs::Error>,
{
  loop {
    if should_exit.load(Ordering::Relaxed) {
      break;
    }
    match poll() {
      Ok(_) => continue,
      Err(e) => {
        if e.kind() == libbpf_rs::ErrorKind::Interrupted {
          continue;
        } else {
          panic!("Failed to poll ringbuf: {e}");
        }
      }
    }
  }
}

fn attach_kprobes_without_syscall_wrappers(skel: &mut TracexecSystemSkel) -> libbpf_rs::Result<()> {
  skel.links.sys_execve_kprobe = Some(
    skel
      .progs
      .sys_execve_kprobe
      .attach_kprobe(false, "__se_sys_execve")?,
  );
  skel.links.sys_execveat_kprobe = Some(
    skel
      .progs
      .sys_execveat_kprobe
      .attach_kprobe(false, "__se_sys_execveat")?,
  );
  skel.links.sys_exit_execve_kretprobe = Some(
    skel
      .progs
      .sys_exit_execve_kretprobe
      .attach_kprobe(true, "__se_sys_execve")?,
  );
  skel.links.sys_exit_execveat_kretprobe = Some(
    skel
      .progs
      .sys_exit_execveat_kretprobe
      .attach_kprobe(true, "__se_sys_execveat")?,
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    env,
    mem::MaybeUninit,
    path::PathBuf,
    sync::{
      Arc,
      atomic::{
        AtomicBool,
        Ordering,
      },
    },
  };

  use enumflags2::BitFlags;
  use serial_test::file_serial;
  use tokio::sync::mpsc::unbounded_channel;
  use tracexec_core::printer::{
    ColorLevel,
    EnvPrintFormat,
    FdPrintFormat,
    PrinterArgs,
  };

  use super::*;

  fn test_printer_args() -> PrinterArgs {
    PrinterArgs {
      trace_comm: false,
      trace_argv: true,
      trace_env: EnvPrintFormat::None,
      trace_fd: FdPrintFormat::None,
      trace_cwd: false,
      print_cmdline: false,
      successful_only: false,
      trace_interpreter: false,
      trace_filename: true,
      decode_errno: false,
      color: ColorLevel::Less,
      stdio_in_cmdline: false,
      fd_in_cmdline: false,
      hide_cloexec_fds: false,
      inline_timestamp_format: None,
    }
  }

  fn test_baseline() -> Arc<BaselineInfo> {
    Arc::new(BaselineInfo::new().expect("failed to capture baseline info"))
  }

  fn find_in_path(bin: &str) -> PathBuf {
    env::var_os("PATH")
      .and_then(|paths| {
        env::split_paths(&paths)
          .filter_map(|dir| {
            let full_path = dir.join(bin);
            if full_path.is_file() {
              Some(full_path)
            } else {
              None
            }
          })
          .next()
      })
      .unwrap_or_else(|| PathBuf::from(bin))
  }

  fn run_ebpf_and_collect(
    cmd: Vec<String>,
    filter: BitFlags<TracerEventDetailsKind>,
  ) -> Vec<TracerMessage> {
    let baseline = test_baseline();
    let printer = Printer::new(test_printer_args(), baseline.clone());
    let (tx, mut rx) = unbounded_channel();
    let tracer = TracerBuilder::new()
      .printer(printer)
      .baseline(baseline)
      .mode(TracerMode::Log { foreground: false })
      .filter(filter)
      .tracer_tx(tx)
      .build_ebpf();
    let mut obj = MaybeUninit::uninit();
    let running = tracer
      .spawn(&cmd, &mut obj, None)
      .expect("failed to spawn eBPF tracer");
    running.run_until_exit();
    let mut msgs = Vec::new();
    while let Ok(msg) = rx.try_recv() {
      msgs.push(msg);
    }
    msgs
  }

  #[test]
  fn build_ebpf_defaults_filter_to_all() {
    let baseline = test_baseline();
    let printer = Printer::new(test_printer_args(), baseline.clone());
    let tracer = TracerBuilder::new()
      .printer(printer)
      .baseline(baseline)
      .mode(TracerMode::Log { foreground: false })
      .build_ebpf();
    assert_eq!(tracer.filter, BitFlags::all());
  }

  #[test]
  fn build_ebpf_respects_filter_and_mode() {
    let baseline = test_baseline();
    let printer = Printer::new(test_printer_args(), baseline.clone());
    let filter = BitFlags::from_flag(TracerEventDetailsKind::Exec)
      | BitFlags::from_flag(TracerEventDetailsKind::TraceeExit);
    let tracer = TracerBuilder::new()
      .printer(printer)
      .baseline(baseline)
      .mode(TracerMode::Log { foreground: true })
      .filter(filter)
      .build_ebpf();
    assert_eq!(tracer.filter, filter);
    assert_eq!(tracer.mode, TracerMode::Log { foreground: true });
  }

  #[test]
  fn run_until_exit_short_circuits_when_flag_set() {
    let flag = AtomicBool::new(true);
    let mut polls = 0;
    run_until_exit_impl(&flag, || {
      polls += 1;
      Ok(())
    });
    assert_eq!(polls, 0);
  }

  #[test]
  fn run_until_exit_ignores_interrupted() {
    let flag = AtomicBool::new(false);
    let mut polls = 0;
    run_until_exit_impl(&flag, || {
      polls += 1;
      if polls == 1 {
        Err(libbpf_rs::Error::from_raw_os_error(libc::EINTR))
      } else {
        flag.store(true, Ordering::Relaxed);
        Ok(())
      }
    });
    assert_eq!(polls, 2);
  }

  #[test]
  #[file_serial(bpf)]
  #[ignore = "root"]
  fn ebpf_tracer_emits_exec_and_exit_events() {
    let true_path = find_in_path("true");
    let filter = BitFlags::from_flag(TracerEventDetailsKind::Exec)
      | BitFlags::from_flag(TracerEventDetailsKind::TraceeExit);
    let events = run_ebpf_and_collect(vec![true_path.to_string_lossy().to_string()], filter);
    let mut saw_exec = false;
    let mut saw_exit = false;
    for event in events {
      if let TracerMessage::Event(TracerEvent { details, .. }) = event {
        match details {
          TracerEventDetails::Exec(exec) => {
            saw_exec = true;
            assert_eq!(exec.syscall, ExecSyscall::Execve);
            assert_eq!(exec.exec_pid, exec.pid);
          }
          TracerEventDetails::TraceeExit { .. } => saw_exit = true,
          _ => {}
        }
      }
    }
    assert!(saw_exec, "expected at least one exec event");
    assert!(saw_exit, "expected a tracee exit event");
  }

  #[test]
  #[file_serial(bpf)]
  #[ignore = "root"]
  fn ebpf_tracer_reports_signal_exit() {
    let sh_path = find_in_path("sh");
    let filter = BitFlags::from_flag(TracerEventDetailsKind::TraceeExit);
    let events = run_ebpf_and_collect(
      vec![
        sh_path.to_string_lossy().to_string(),
        "-c".to_string(),
        "kill -TERM $$".to_string(),
      ],
      filter,
    );
    let mut saw_signal_exit = false;
    for event in events {
      if let TracerMessage::Event(TracerEvent {
        details: TracerEventDetails::TraceeExit { signal, .. },
        ..
      }) = event
      {
        saw_signal_exit = signal == Some(Signal::Standard(nix::sys::signal::Signal::SIGTERM));
      }
    }
    assert!(saw_signal_exit, "expected a signal-based exit event");
  }
}
