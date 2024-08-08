use std::{thread::sleep, time::Duration};

use color_eyre::eyre::bail;
use libbpf_rs::skel::{OpenSkel, Skel, SkelBuilder};
use nix::libc;

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
  let open_skel = skel_builder.open()?;
  let mut skel = open_skel.load()?;
  skel.attach()?;
  sleep(Duration::from_secs(1000));
  Ok(())
}
