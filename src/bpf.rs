use std::{mem::MaybeUninit, time::Duration};

use color_eyre::eyre::bail;
use interface::EventHeader;
use libbpf_rs::{
  skel::{OpenSkel, Skel, SkelBuilder},
  RingBufferBuilder,
};
use nix::libc;

pub mod skel {
  include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/tracexec_system.skel.rs"
  ));
}

mod interface;

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
  let open_skel = skel_builder.open(&mut obj)?;
  let mut skel = open_skel.load()?;
  skel.attach()?;
  let events = skel.maps.events;
  let mut builder = RingBufferBuilder::new();
  builder.add(&events, |data| {
    let header: EventHeader = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    match header.kind {
      interface::EventType::Sysenter => todo!(),
      interface::EventType::Sysexit => todo!(),
      interface::EventType::String => {
        let header_len = size_of::<EventHeader>();
        let string = String::from_utf8_lossy(&data[header_len..]);
        eprintln!("String for EID: {}: {}", header.eid, string);
      }
    }
    0
  })?;
  let rb = builder.build()?;
  loop {
    rb.poll(Duration::from_millis(1000))?;
  }
}
