use std::{
  borrow::Cow,
  collections::HashMap,
  ffi::CStr,
  mem::MaybeUninit,
  sync::{Arc, OnceLock, RwLock},
  time::Duration,
};

use arcstr::ArcStr;
use color_eyre::eyre::bail;
use libbpf_rs::{
  num_possible_cpus,
  skel::{OpenSkel, Skel, SkelBuilder},
  RingBufferBuilder,
};
use nix::libc;
use skel::types::{event_header, event_type, exec_event};

use crate::cache::StringCache;

pub mod skel {
  include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/tracexec_system.skel.rs"
  ));
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

pub fn experiment() -> color_eyre::Result<()> {
  let skel_builder = skel::TracexecSystemSkelBuilder::default();
  bump_memlock_rlimit()?;
  let mut obj = MaybeUninit::uninit();
  let mut open_skel = skel_builder.open(&mut obj)?;
  let ncpu = num_possible_cpus()?.try_into().expect("Too many cores!");
  open_skel.maps.rodata_data.config.max_num_cpus = ncpu;
  open_skel.maps.cache.set_max_entries(ncpu)?;
  let mut skel = open_skel.load()?;
  skel.attach()?;
  let events = skel.maps.events;
  let mut builder = RingBufferBuilder::new();
  let strings: Arc<RwLock<HashMap<u64, Vec<(ArcStr, u32)>>>> =
    Arc::new(RwLock::new(HashMap::new()));
  builder.add(&events, move |data| {
    assert!(data.len() > size_of::<event_header>(), "data too short: {data:?}");
    let header: event_header = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    match unsafe { header.r#type.assume_init() } {
      event_type::SYSENTER_EVENT => unreachable!(),
      event_type::SYSEXIT_EVENT => {
        assert_eq!(data.len(), size_of::<exec_event>());
        let event: exec_event = unsafe { std::ptr::read(data.as_ptr() as *const _) };
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
        let header: event_header = unsafe { std::ptr::read(data.as_ptr() as *const _) };
        let string = String::from_utf8_lossy(
          CStr::from_bytes_with_nul(&data[header_len..])
            .unwrap()
            .to_bytes(),
        );
        let cached = cached_cow(string);
        let mut lock_guard = strings.write().unwrap();
        let strings = lock_guard.entry(header.eid).or_default();
        strings.push((cached, header.flags));
        drop(lock_guard);
      }
      event_type::FD_EVENT => {},
    }
    0
  })?;
  let rb = builder.build()?;
  loop {
    rb.poll(Duration::from_millis(1000))?;
  }
}

fn cached_cow(cow: Cow<str>) -> ArcStr {
  let cache = CACHE.get_or_init(|| Arc::new(RwLock::new(StringCache::new())));
  match cow {
    Cow::Borrowed(s) => cache.write().unwrap().get_or_insert(s),
    Cow::Owned(s) => cache.write().unwrap().get_or_insert_owned(s),
  }
}

static CACHE: OnceLock<Arc<RwLock<StringCache>>> = OnceLock::new();
