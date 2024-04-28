use std::ffi::CString;

use nix::unistd::execv;

fn main() {
  let arg1 = CString::new(std::env::args().nth(1).unwrap()).unwrap();
  execv(&arg1, &[&arg1]).unwrap();
}
