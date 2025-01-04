use nix::libc::execve;

fn main() {
  let i = std::env::args()
    .next()
    .unwrap_or_else(|| "0".to_string())
    .parse()
    .unwrap_or(0);
  if i > 3 {
    return;
  }
  let arg0 = format!("{}\0", i + 1);
  unsafe {
    execve(
      c"/proc/self/exe".as_ptr(),
      [arg0.as_ptr() as *const nix::libc::c_char, std::ptr::null()].as_ptr(),
      b"asfdasfafadfasdfgsadfg".as_ptr().cast(),
    );
  }
}
