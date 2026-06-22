//! Open pseudo-filesystem fds, clear CLOEXEC, then exec ourselves.
//!
//! The eBPF backend inspects fds at exec time. Re-execing after opening these
//! descriptors verifies that synthetic d_dname-backed files survive across the
//! exec boundary and are reported in the post-open process image.

use std::{
  ffi::CString,
  os::fd::RawFd,
  ptr,
};

use nix::libc;

fn keep_across_exec(fd: RawFd) -> Option<RawFd> {
  if fd < 0 {
    return None;
  }

  let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
  if flags < 0 {
    return Some(fd);
  }

  unsafe {
    libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
  }
  Some(fd)
}

fn open_pipe(fds: &mut Vec<RawFd>) {
  let mut pipe_fds = [-1; 2];
  if unsafe { libc::pipe2(pipe_fds.as_mut_ptr(), 0) } == 0 {
    fds.extend(pipe_fds.into_iter().filter_map(keep_across_exec));
  }
}

fn open_socketpair(fds: &mut Vec<RawFd>) {
  let mut socket_fds = [-1; 2];
  if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, socket_fds.as_mut_ptr()) } == 0
  {
    fds.extend(socket_fds.into_iter().filter_map(keep_across_exec));
  }
}

fn open_anon_inode(fds: &mut Vec<RawFd>) {
  if let Some(fd) = keep_across_exec(unsafe { libc::eventfd(0, 0) }) {
    fds.push(fd);
  }

  if let Some(fd) = keep_across_exec(unsafe { libc::epoll_create1(0) }) {
    fds.push(fd);
  }
}

fn open_namespace(fds: &mut Vec<RawFd>) {
  let path = c"/proc/self/ns/mnt";
  if let Some(fd) = keep_across_exec(unsafe { libc::open(path.as_ptr(), libc::O_RDONLY) }) {
    fds.push(fd);
  }
}

fn open_pidfd(fds: &mut Vec<RawFd>) {
  #[cfg(target_os = "linux")]
  {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) };
    if let Some(fd) = keep_across_exec(fd as RawFd) {
      fds.push(fd);
    }
  }
}

fn exec_self() -> ! {
  let exe = c"/proc/self/exe";
  let argv0 = CString::new("special-fds-exec").unwrap();
  let child = CString::new("child").unwrap();
  let argv = [argv0.as_ptr(), child.as_ptr(), ptr::null()];

  unsafe {
    libc::execv(exe.as_ptr(), argv.as_ptr());
    libc::_exit(127);
  }
}

fn main() {
  if std::env::args().nth(1).as_deref() == Some("child") {
    return;
  }

  let mut fds = Vec::new();
  open_pipe(&mut fds);
  open_socketpair(&mut fds);
  open_anon_inode(&mut fds);
  open_namespace(&mut fds);
  open_pidfd(&mut fds);

  exec_self();
}
