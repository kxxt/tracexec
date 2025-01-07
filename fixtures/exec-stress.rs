use std::ffi::CString;

use nix::unistd::execv;
use rand::{distributions::Alphanumeric, Rng};

// A stress test.
// It will exec itself with random strings as arguments for n times
fn main() {
  let mut args = std::env::args().skip(1);
  let n: u64 = args
    .next()
    .expect("missing n")
    .parse()
    .expect("cannot parse n");
  if n == 0 {
    // nix::sys::signal::raise(nix::sys::signal::SIGSTOP);
    return;
  }
  let mut rand_args = vec![
    CString::new("Hello").unwrap(),
    CString::new((n - 1).to_string()).unwrap(),
  ];
  rand_args.extend((0..10).map(|_| unsafe {
    CString::from_vec_unchecked(
      rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(512)
        .chain(Some(0))
        .collect::<Vec<u8>>(),
    )
  }));
  execv(c"/proc/self/exe", &rand_args).unwrap();
}
