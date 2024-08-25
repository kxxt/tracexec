use std::{
  borrow::Cow,
  collections::HashMap,
  ffi::{CStr, CString},
  io::stdin,
  iter::repeat,
  mem::MaybeUninit,
  os::{fd::RawFd, unix::{fs::MetadataExt, process::CommandExt}},
  sync::{Arc, OnceLock, RwLock},
  time::Duration,
};

use arcstr::ArcStr;
use color_eyre::eyre::bail;
use interface::exec_event_flags_USERSPACE_DROP_MARKER;
use libbpf_rs::{
  num_possible_cpus,
  skel::{OpenSkel, Skel, SkelBuilder},
  RingBufferBuilder,
};
use nix::{
  errno::Errno,
  fcntl::OFlag,
  libc::{self, c_int},
  sys::{
    signal::{kill, raise, Signal},
    wait::{waitpid, WaitPidFlag, WaitStatus},
  },
  unistd::{
    fork, getpid, initgroups, setpgid, setresgid, setresuid, tcsetpgrp, ForkResult, Gid, Uid, User,
  },
};
use skel::types::{event_header, event_type, exec_event, fd_event};

use crate::{
  cache::StringCache,
  cli::args::ModifierArgs,
  cmdbuilder::CommandBuilder,
  printer::PrinterOut,
  proc::{FileDescriptorInfo, FileDescriptorInfoCollection},
  pty,
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

fn bump_memlock_rlimit() -> color_eyre::Result<()> {
  let rlimit = libc::rlimit {
    rlim_cur: 128 << 20,
    rlim_max: 128 << 20,
  };

  if unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlimit) } != 0 {
    bail!("Failed to increase rlimit for memlock");
  }

  Ok(())
}

pub fn run(
  output: Box<PrinterOut>,
  args: Vec<String>,
  user: Option<User>,
  modifier: ModifierArgs,
) -> color_eyre::Result<()> {
  let skel_builder = skel::TracexecSystemSkelBuilder::default();
  bump_memlock_rlimit()?;
  let mut obj = MaybeUninit::uninit();
  let mut open_skel = skel_builder.open(&mut obj)?;
  let ncpu = num_possible_cpus()?.try_into().expect("Too many cores!");
  open_skel.maps.rodata_data.config.max_num_cpus = ncpu;
  open_skel.maps.cache.set_max_entries(ncpu)?;
  // tracexec runs in the same pid namespace with the tracee
  let pid_ns_ino = std::fs::metadata("/proc/self/ns/pid")?.ino();
  let skel = if !args.is_empty() {
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
        open_skel.maps.rodata_data.config.follow_fork = MaybeUninit::new(true);
        open_skel.maps.rodata_data.config.tracee_pid = child.as_raw();
        open_skel.maps.rodata_data.config.tracee_pidns_inum = pid_ns_ino as u32;
        let mut skel = open_skel.load()?;
        skel.attach()?;
        match waitpid(child, Some(WaitPidFlag::WSTOPPED))? {
          terminated @ WaitStatus::Exited(_, _) | terminated @ WaitStatus::Signaled(_, _, _) => {
            panic!("Child exited abnormally before tracing is started: status: {terminated:?}");
          }
          WaitStatus::Stopped(_, _) => kill(child, Signal::SIGCONT)?,
          _ => unreachable!("Invalid wait status!"),
        }
        skel
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
    skel
  };
  let events = skel.maps.events;
  let mut builder = RingBufferBuilder::new();
  let strings: Arc<RwLock<HashMap<u64, Vec<(ArcStr, u32)>>>> =
    Arc::new(RwLock::new(HashMap::new()));
  let fdinfo_map: Arc<RwLock<HashMap<u64, FileDescriptorInfoCollection>>> =
    Arc::new(RwLock::new(HashMap::new()));
  let mut eid = 0;
  builder.add(&events, move |data| {
    assert!(
      data.len() > size_of::<event_header>(),
      "data too short: {data:?}"
    );
    let header: event_header = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    match unsafe { header.r#type.assume_init() } {
      event_type::SYSENTER_EVENT => unreachable!(),
      event_type::SYSEXIT_EVENT => {
        if header.eid > eid {
          eprintln!(
            "warning: inconsistent event id counter: local = {eid}, kernel = {}. Possible event loss!",
            header.eid
          );
          // reset local counter
          eid = header.eid + 1;
        } else if header.eid < eid {
          // This should never happen
          panic!("inconsistent event id counter: local = {} > kernel = {}.", eid, header.eid);
        } else {
          // increase local counter for next event
          eid += 1;
        }
        assert_eq!(data.len(), size_of::<exec_event>());
        let event: exec_event = unsafe { std::ptr::read(data.as_ptr() as *const _) };
        if event.ret != 0 && modifier.successful_only {
          return 0;
        }
        eprint!(
          "{} exec {} argv ",
          String::from_utf8_lossy(&event.comm),
          String::from_utf8_lossy(&event.base_filename),
        );
        for i in 0..event.count[0] {
          eprint!(
            "{:?} ",
            strings.read().unwrap().get(&event.header.eid).unwrap()[i as usize].0
          );
        }
        eprint!("envp ");
        for i in event.count[0]..(event.count[0] + event.count[1]) {
          eprint!(
            "{:?} ",
            strings.read().unwrap().get(&event.header.eid).unwrap()[i as usize].0
          );
        }
        eprintln!("= {}", event.ret);
      }
      event_type::STRING_EVENT => {
        let header_len = size_of::<event_header>();
        let string = utf8_lossy_cow_from_bytes_with_nul(&data[header_len..]);
        let cached = cached_cow(string);
        let mut lock_guard = strings.write().unwrap();
        let strings = lock_guard.entry(header.eid).or_default();
        // Catch event drop
        if strings.len() != header.id as usize {
          // Insert placeholders for dropped events
          let placeholder = arcstr::literal!("[dropped from ringbuf]");
          let dropped_event = (placeholder, exec_event_flags_USERSPACE_DROP_MARKER);
          strings.extend(repeat(dropped_event).take(header.id as usize - strings.len()));
          debug_assert_eq!(strings.len(), header.id as usize);
        }
        strings.push((cached, header.flags));
        drop(lock_guard);
      }
      event_type::FD_EVENT => {
        assert_eq!(data.len(), size_of::<fd_event>());
        let event: fd_event = unsafe { std::ptr::read(data.as_ptr() as *const _) };
        let fdinfo = FileDescriptorInfo {
          fd: event.fd as RawFd,
          path: cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.path)),
          pos: 0, // TODO
          flags: OFlag::from_bits_retain(event.flags as c_int),
          mnt_id: 0,                                    // TODO
          ino: 0,                                       // TODO
          mnt: arcstr::literal!("[tracexec: unknown]"), // TODO
          extra: vec![arcstr::literal!("")],            // TODO
        };
        let mut lock_guard = fdinfo_map.write().unwrap();
        let fdc = lock_guard.entry(header.eid).or_default();
        fdc.fdinfo.insert(event.fd as RawFd, fdinfo);
        drop(lock_guard);
      }
    }
    0
  })?;
  let rb = builder.build()?;
  loop {
    rb.poll(Duration::from_millis(1000))?;
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
