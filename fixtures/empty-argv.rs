use nix::libc::execve;

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
    execve(
      c"/proc/self/exe".as_ptr(),
      std::ptr::null(),
      [env0.as_ptr() as *const nix::libc::c_char, std::ptr::null()].as_ptr(),
    );
  }
}
