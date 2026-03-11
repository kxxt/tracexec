use nix::libc::{AT_EMPTY_PATH, execveat};

fn main() {
  let i = std::env::var("COUNTER")
    .unwrap_or_else(|_| "0".to_string())
    .parse()
    .unwrap_or(0);
  if i > 3 {
    return;
  }
  let env0 = format!("COUNTER={}\0", i + 1);
  unsafe {
    execveat(
      AT_FDCWD,
      c"/proc/self/exe".as_ptr(),
      std::ptr::null(),
      [env0.as_ptr() as *mut nix::libc::c_char, std::ptr::null()].as_ptr() as _,
      0
    );
  }
}
