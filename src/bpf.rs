use std::time::Duration;

use color_eyre::eyre::bail;
use interface::StringEntryHeader;
use libbpf_rs::{
  skel::{OpenSkel, Skel, SkelBuilder},
  MapHandle, RingBufferBuilder,
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
  let open_skel = skel_builder.open()?;
  let mut skel = open_skel.load()?;
  skel.attach()?;
  let string_io = MapHandle::from_map_id(skel.maps().string_io().info()?.info.id)?;
  let mut builder = RingBufferBuilder::new();
  builder.add(&string_io, |data| {
    let header: StringEntryHeader = unsafe { std::ptr::read(data.as_ptr() as *const _) };
    let header_len = size_of::<StringEntryHeader>();
    let string = String::from_utf8_lossy(&data[header_len..]);
    eprintln!(
      "PID: {}, EID: {}, String: {}",
      header.pid, header.eid, string
    );
    0
  })?;
  let rb = builder.build()?;
  loop {
    rb.poll(Duration::from_millis(1000))?;
  }
}
