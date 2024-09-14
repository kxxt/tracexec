use std::{
  borrow::Cow,
  cell::RefCell,
  collections::{HashMap, HashSet},
  ffi::{CStr, CString},
  io::stdin,
  iter::repeat,
  mem::MaybeUninit,
  os::{
    fd::RawFd,
    unix::{fs::MetadataExt, process::CommandExt},
  },
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock, RwLock,
  },
  time::Duration,
};

use arcstr::ArcStr;
use color_eyre::Section;
use enumflags2::BitFlag;
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
  libc::{self, c_int, SYS_execve, SYS_execveat, AT_FDCWD},
  sys::{
    signal::{kill, raise, Signal},
    wait::{waitpid, WaitPidFlag, WaitStatus},
  },
  unistd::{
    fork, getpid, initgroups, setpgid, setresgid, setresuid, tcsetpgrp, ForkResult, Gid, Pid, Uid,
    User,
  },
};
use skel::{
  types::{
    event_type, exec_event, fd_event, path_event, path_segment_event, tracexec_event_header,
  },
  TracexecSystemSkel,
};

use crate::{
  cache::StringCache,
  cli::{args::ModifierArgs, options::Color, Cli, EbpfCommand},
  cmdbuilder::CommandBuilder,
  event::{FriendlyError, OutputMsg},
  printer::{Printer, PrinterArgs, PrinterOut},
  proc::{cached_string, parse_failiable_envp, BaselineInfo, FileDescriptorInfo},
  pty,
  tracer::state::ExecData,
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

fn spawn<'obj>(
  args: &[String],
  user: Option<User>,
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
  let (skel, child) = if !args.is_empty() {
    let mut cmd = CommandBuilder::new(&args[0]);
    cmd.args(args.iter().skip(1));
    cmd.cwd(std::env::current_dir()?);
    let mut cmd = cmd.as_command()?;
    match unsafe { fork()? } {
      ForkResult::Parent { child } => {
        match tcsetpgrp(stdin(), child) {
          Ok(_) => {}
          Err(Errno::ENOTTY) => {
            eprintln!("tcsetpgrp failed: ENOTTY");
          }
          r => r?,
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
        let me = getpid();
        setpgid(me, me)?;

        // Wait for eBPF program to load
        raise(Signal::SIGSTOP)?;

        if let Some(user) = &user {
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

pub struct EbpfTracer {
  cmd: Vec<String>,
  user: Option<User>,
  modifier: ModifierArgs,
  printer: Arc<Printer>,
  baseline: Arc<BaselineInfo>,
}

impl EbpfTracer {
  pub fn new(
    cmd: Vec<String>,
    user: Option<User>,
    modifier: ModifierArgs,
    printer: Arc<Printer>,
    baseline: Arc<BaselineInfo>,
  ) -> Self {
    Self {
      cmd,
      user,
      modifier,
      printer,
      baseline,
    }
  }

  pub fn spawn<'obj>(
    self,
    obj: &'obj mut MaybeUninit<OpenObject>,
    output: Option<Box<PrinterOut>>,
  ) -> color_eyre::Result<RunningEbpfTracer<'obj>> {
    let (skel, _child) = spawn(&self.cmd, self.user, obj)?;
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
        data.len() >= size_of::<tracexec_event_header>(),
        "data too short: {data:?}"
      );
      let header: &tracexec_event_header = unsafe { &*(data.as_ptr() as *const _) };
      match unsafe { header.r#type.assume_init() } {
        event_type::SYSENTER_EVENT => unreachable!(),
        event_type::SYSEXIT_EVENT => {
          if header.eid > eid {
            // There are some lost events
            // In some cases the events are not really lost but sent out of order because of parallism
            eprintln!(
              "warning: inconsistent event id counter: local = {eid}, kernel = {}. Possible event loss!",
              header.eid
            );
            lost_events.borrow_mut().extend(eid..header.eid);
            // reset local counter
            eid = header.eid + 1;
          } else if header.eid < eid {
            // This should only happen for lost events
            if lost_events.borrow_mut().remove(&header.eid) {
              // do nothing
            } else {
              panic!("inconsistent event id counter: local = {}, kernel = {}.", eid, header.eid);
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
          let filename = if event.syscall_nr == SYS_execve as i32 {
            base_filename
          } else if event.syscall_nr == SYS_execveat as i32 {
            if base_filename.is_ok_and(|s| s.starts_with('/')) {
              base_filename
            } else {
              match event.fd  {
                AT_FDCWD => {
                  cwd.join(base_filename)
                },
                fd => {
                  // Check if it is a valid fd
                  if let Some(fdinfo) = storage.fdinfo_map.get(fd) {
                    fdinfo.path.clone().join(base_filename)
                  } else {
                    OutputMsg::PartialOk(cached_string(format!("[err: invalid fd: {fd}]/{base_filename}")))
                  }
                }
              }
            }
          } else {
            unreachable!()
          };
          let exec_data = ExecData::new(
            filename,
            Ok(argv), Ok(parse_failiable_envp(envp)),
            cwd, None, storage.fdinfo_map);
          self.printer.print_exec_trace(Pid::from_raw(header.pid), cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.comm)), event.ret, &exec_data, &self.baseline.env, &self.baseline.cwd).unwrap();
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
              "pipefs" =>OutputMsg::Ok(cached_string(format!("pipe:[{}]", event.ino))),
              "sockfs" =>OutputMsg::Ok(cached_string(format!("socket:[{}]", event.ino))),
              _ => storage.paths.get(&event.path_id).unwrap().to_owned().into()
          };
          let fdinfo = FileDescriptorInfo {
            fd: event.fd as RawFd,
            path,
            pos: 0, // TODO
            flags: OFlag::from_bits_retain(event.flags as c_int),
            mnt_id: event.mnt_id,
            ino: event.ino,
            mnt: arcstr::literal!("[tracexec: unknown]"), // TODO
            extra: vec![arcstr::literal!("")],            // TODO
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
          path.segments.push(OutputMsg::Ok(cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.segment))));
        }
        event_type::EXIT_EVENT => {
          should_exit.store(true, Ordering::Relaxed);
        }
      }
      0
    }})?;
    printer_clone.init_thread_local(output);
    let rb = builder.build()?;
    Ok(RunningEbpfTracer {
      rb,
      should_exit,
      skel,
    })
  }
}

// TODO: we should start polling the ringbuffer before program load

pub struct RunningEbpfTracer<'obj> {
  rb: RingBuffer<'obj>,
  should_exit: Arc<AtomicBool>,
  // The eBPF program gets unloaded on skel drop
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

pub fn run(command: EbpfCommand, user: Option<User>, color: Color) -> color_eyre::Result<()> {
  let mut obj = Box::leak(Box::new(MaybeUninit::uninit()));
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
      let tracer = EbpfTracer::new(cmd, user, modifier_args, printer, baseline);
      let running_tracer = tracer.spawn(&mut obj, Some(output))?;
      running_tracer.run_until_exit();
      Ok(())
    }
    EbpfCommand::Tui {
      cmd,
      modifier_args,
      tracer_event_args,
      tui_args,
    } => Ok(()),
    EbpfCommand::Collect {} => todo!(),
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
