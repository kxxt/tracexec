use std::os::unix::ffi::OsStrExt;

use atoi::atoi;
use color_eyre::eyre::bail;

pub mod breakpoint;
pub mod cache;
pub mod cli;
pub mod cmdbuilder;
pub mod event;
pub mod export;
pub mod output;
pub mod primitives;
pub mod printer;
pub mod proc;
pub mod pty;
pub mod timestamp;
pub mod tracee;
pub mod tracer;

pub fn is_current_kernel_ge(min_support: (u32, u32)) -> color_eyre::Result<bool> {
  let utsname = nix::sys::utsname::uname()?;
  let kstr = utsname.release().as_bytes();
  let pos = kstr.iter().position(|&c| c != b'.' && !c.is_ascii_digit());
  let kver = pos.map_or(kstr, |pos| kstr.split_at(pos).0);
  let mut kvers = kver.split(|&c| c == b'.');
  let Some(major) = kvers.next().and_then(atoi::<u32>) else {
    bail!("Failed to parse kernel major ver!")
  };
  let Some(minor) = kvers.next().and_then(atoi::<u32>) else {
    bail!("Failed to parse kernel minor ver!")
  };
  Ok((major, minor) >= min_support)
}

#[cfg(test)]
mod test {
  use crate::is_current_kernel_ge;

  #[test]
  fn test_is_current_kernel_ge() {
    assert!(is_current_kernel_ge((3, 0)).unwrap())
  }
}
