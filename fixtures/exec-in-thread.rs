use std::{
  env,
  ffi::CString,
  thread,
};

#[allow(unreachable_code)]
fn main() {
  if std::env::args().next().as_deref() == Some("") {
    return;
  }

  let exe_path = env::current_exe().expect("Failed to get current executable path");
  let c_path = CString::new(exe_path.to_string_lossy().as_bytes())
    .expect("Failed to create CString from path");

  // Spawn a thread and perform exec from within it
  let handle = thread::spawn(move || {
    nix::unistd::execve(&c_path, &[c""], &[c""]).unwrap();
  });

  let _ = handle.join();
}
