// MIT License

// Copyright (c) 2018 Wez Furlong
// Copyright (c) 2024 Levi Zim

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Modified from https://github.com/wez/wezterm/tree/main/pty

#![allow(unused)]

use color_eyre::eyre::{Error, bail};
use filedescriptor::FileDescriptor;
use nix::libc::{self, pid_t, winsize};
use nix::unistd::{Pid, dup2, execv, fork};
use std::cell::RefCell;
use std::ffi::{CStr, CString, OsStr};

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{io, mem, ptr};

// use downcast_rs::{impl_downcast, Downcast};

use std::io::Result as IoResult;

use crate::cmdbuilder::CommandBuilder;

/// Represents the size of the visible display area in the pty
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
  /// The number of lines of text
  pub rows: u16,
  /// The number of columns of text
  pub cols: u16,
  /// The width of a cell in pixels.  Note that some systems never
  /// fill this value and ignore it.
  pub pixel_width: u16,
  /// The height of a cell in pixels.  Note that some systems never
  /// fill this value and ignore it.
  pub pixel_height: u16,
}

impl Default for PtySize {
  fn default() -> Self {
    Self {
      rows: 24,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    }
  }
}

/// Represents the master/control end of the pty
pub trait MasterPty: Send {
  /// Inform the kernel and thus the child process that the window resized.
  /// It will update the winsize information maintained by the kernel,
  /// and generate a signal for the child to notice and update its state.
  fn resize(&self, size: PtySize) -> Result<(), Error>;
  /// Retrieves the size of the pty as known by the kernel
  fn get_size(&self) -> Result<PtySize, Error>;
  /// Obtain a readable handle; output from the slave(s) is readable
  /// via this stream.
  fn try_clone_reader(&self) -> Result<Box<dyn std::io::Read + Send>, Error>;
  /// Obtain a writable handle; writing to it will send data to the
  /// slave end.
  /// Dropping the writer will send EOF to the slave end.
  /// It is invalid to take the writer more than once.
  fn take_writer(&self) -> Result<Box<dyn std::io::Write + Send>, Error>;

  /// If applicable to the type of the tty, return the local process id
  /// of the process group or session leader
  #[cfg(unix)]
  fn process_group_leader(&self) -> Option<libc::pid_t>;

  /// If get_termios() and process_group_leader() are both implemented and
  /// return Some, then as_raw_fd() should return the same underlying fd
  /// associated with the stream. This is to enable applications that
  /// "know things" to query similar information for themselves.
  #[cfg(unix)]
  fn as_raw_fd(&self) -> Option<RawFd>;

  #[cfg(unix)]
  fn tty_name(&self) -> Option<std::path::PathBuf>;

  /// If applicable to the type of the tty, return the termios
  /// associated with the stream
  #[cfg(unix)]
  fn get_termios(&self) -> Option<nix::sys::termios::Termios> {
    None
  }
}

/// Represents a child process spawned into the pty.
/// This handle can be used to wait for or terminate that child process.
pub trait Child: std::fmt::Debug + ChildKiller + Send {
  /// Poll the child to see if it has completed.
  /// Does not block.
  /// Returns None if the child has not yet terminated,
  /// else returns its exit status.
  fn try_wait(&mut self) -> IoResult<Option<ExitStatus>>;
  /// Blocks execution until the child process has completed,
  /// yielding its exit status.
  fn wait(&mut self) -> IoResult<ExitStatus>;
  /// Returns the process identifier of the child process,
  /// if applicable
  fn process_id(&self) -> Pid;
}

/// Represents the ability to signal a Child to terminate
pub trait ChildKiller: std::fmt::Debug + Send {
  /// Terminate the child process
  fn kill(&mut self) -> IoResult<()>;

  /// Clone an object that can be split out from the Child in order
  /// to send it signals independently from a thread that may be
  /// blocked in `.wait`.
  fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync>;
}

/// Represents the exit status of a child process.
#[derive(Debug, Clone)]
pub struct ExitStatus {
  code: u32,
  signal: Option<String>,
}

impl ExitStatus {
  /// Construct an ExitStatus from a process return code
  pub fn with_exit_code(code: u32) -> Self {
    Self { code, signal: None }
  }

  /// Construct an ExitStatus from a signal name
  pub fn with_signal(signal: &str) -> Self {
    Self {
      code: 1,
      signal: Some(signal.to_string()),
    }
  }

  /// Returns true if the status indicates successful completion
  pub fn success(&self) -> bool {
    match self.signal {
      None => self.code == 0,
      Some(_) => false,
    }
  }

  /// Returns the exit code that this ExitStatus was constructed with
  pub fn exit_code(&self) -> u32 {
    self.code
  }

  /// Returns the signal if present that this ExitStatus was constructed with
  pub fn signal(&self) -> Option<&str> {
    self.signal.as_deref()
  }
}

impl From<std::process::ExitStatus> for ExitStatus {
  fn from(status: std::process::ExitStatus) -> Self {
    #[cfg(unix)]
    {
      use std::os::unix::process::ExitStatusExt;

      if let Some(signal) = status.signal() {
        let signame = unsafe { libc::strsignal(signal) };
        let signal = if signame.is_null() {
          format!("Signal {signal}")
        } else {
          let signame = unsafe { std::ffi::CStr::from_ptr(signame) };
          signame.to_string_lossy().to_string()
        };

        return Self {
          code: status.code().map(|c| c as u32).unwrap_or(1),
          signal: Some(signal),
        };
      }
    }

    let code = status
      .code()
      .map(|c| c as u32)
      .unwrap_or_else(|| if status.success() { 0 } else { 1 });

    Self { code, signal: None }
  }
}

impl std::fmt::Display for ExitStatus {
  fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
    if self.success() {
      write!(fmt, "Success")
    } else {
      match &self.signal {
        Some(sig) => write!(fmt, "Terminated by {sig}"),
        None => write!(fmt, "Exited with code {}", self.code),
      }
    }
  }
}

pub struct PtyPair {
  // slave is listed first so that it is dropped first.
  // The drop order is stable and specified by rust rfc 1857
  pub slave: UnixSlavePty,
  pub master: UnixMasterPty,
}

/// The `PtySystem` trait allows an application to work with multiple
/// possible Pty implementations at runtime.  This is important on
/// Windows systems which have a variety of implementations.
pub trait PtySystem {
  /// Create a new Pty instance with the window size set to the specified
  /// dimensions.  Returns a (master, slave) Pty pair.  The master side
  /// is used to drive the slave side.
  fn openpty(&self, size: PtySize) -> color_eyre::Result<PtyPair>;
}

impl Child for std::process::Child {
  fn try_wait(&mut self) -> IoResult<Option<ExitStatus>> {
    Self::try_wait(self).map(|s| s.map(Into::into))
  }

  fn wait(&mut self) -> IoResult<ExitStatus> {
    Self::wait(self).map(Into::into)
  }

  fn process_id(&self) -> Pid {
    Pid::from_raw(self.id() as pid_t)
  }
}

#[derive(Debug)]
struct ProcessSignaller {
  pid: Option<Pid>,
}

impl ChildKiller for ProcessSignaller {
  fn kill(&mut self) -> IoResult<()> {
    if let Some(pid) = self.pid {
      let result = unsafe { libc::kill(pid.as_raw(), libc::SIGHUP) };
      if result != 0 {
        return Err(std::io::Error::last_os_error());
      }
    }
    Ok(())
  }

  fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
    Box::new(Self { pid: self.pid })
  }
}

impl ChildKiller for std::process::Child {
  fn kill(&mut self) -> IoResult<()> {
    #[cfg(unix)]
    {
      // On unix, we send the SIGHUP signal instead of trying to kill
      // the process. The default behavior of a process receiving this
      // signal is to be killed unless it configured a signal handler.
      let result = unsafe { libc::kill(self.id() as i32, libc::SIGHUP) };
      if result != 0 {
        return Err(std::io::Error::last_os_error());
      }

      // We successfully delivered SIGHUP, but the semantics of Child::kill
      // are that on success the process is dead or shortly about to
      // terminate.  Since SIGUP doesn't guarantee termination, we
      // give the process a bit of a grace period to shutdown or do whatever
      // it is doing in its signal handler before we proceed with the
      // full on kill.
      for attempt in 0..5 {
        if attempt > 0 {
          std::thread::sleep(std::time::Duration::from_millis(50));
        }

        if let Ok(Some(_)) = self.try_wait() {
          // It completed, so report success!
          return Ok(());
        }
      }

      // it's still alive after a grace period, so proceed with a kill
    }

    Self::kill(self)
  }

  fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
    Box::new(ProcessSignaller {
      pid: Some(self.process_id()),
    })
  }
}

pub fn native_pty_system() -> NativePtySystem {
  NativePtySystem::default()
}

pub type NativePtySystem = UnixPtySystem;

#[derive(Default)]
pub struct UnixPtySystem {}

fn openpty(size: PtySize) -> color_eyre::Result<(UnixMasterPty, UnixSlavePty)> {
  let mut master: RawFd = -1;
  let mut slave: RawFd = -1;

  let mut size = winsize {
    ws_row: size.rows,
    ws_col: size.cols,
    ws_xpixel: size.pixel_width,
    ws_ypixel: size.pixel_height,
  };

  let result = unsafe {
    libc::openpty(
      &mut master,
      &mut slave,
      ptr::null_mut(),
      ptr::null_mut(),
      &size,
    )
  };

  if result != 0 {
    bail!("failed to openpty: {:?}", io::Error::last_os_error());
  }

  let tty_name = tty_name(slave);

  let master = UnixMasterPty {
    fd: PtyFd(unsafe { FileDescriptor::from_raw_fd(master) }),
    took_writer: RefCell::new(false),
    tty_name,
  };
  let slave = UnixSlavePty {
    fd: PtyFd(unsafe { FileDescriptor::from_raw_fd(slave) }),
  };

  // Ensure that these descriptors will get closed when we execute
  // the child process.  This is done after constructing the Pty
  // instances so that we ensure that the Ptys get drop()'d if
  // the cloexec() functions fail (unlikely!).
  cloexec(master.fd.as_raw_fd())?;
  cloexec(slave.fd.as_raw_fd())?;

  Ok((master, slave))
}

impl PtySystem for UnixPtySystem {
  fn openpty(&self, size: PtySize) -> color_eyre::Result<PtyPair> {
    let (master, slave) = openpty(size)?;
    Ok(PtyPair { master, slave })
  }
}

pub struct PtyFd(pub FileDescriptor);
impl std::ops::Deref for PtyFd {
  type Target = FileDescriptor;
  fn deref(&self) -> &FileDescriptor {
    &self.0
  }
}
impl std::ops::DerefMut for PtyFd {
  fn deref_mut(&mut self) -> &mut FileDescriptor {
    &mut self.0
  }
}

impl Read for PtyFd {
  fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
    match self.0.read(buf) {
      Err(ref e) if e.raw_os_error() == Some(libc::EIO) => {
        // EIO indicates that the slave pty has been closed.
        // Treat this as EOF so that std::io::Read::read_to_string
        // and similar functions gracefully terminate when they
        // encounter this condition
        Ok(0)
      }
      x => x,
    }
  }
}

fn tty_name(fd: RawFd) -> Option<PathBuf> {
  let mut buf = vec![0 as std::ffi::c_char; 128];

  loop {
    let res = unsafe { libc::ttyname_r(fd, buf.as_mut_ptr(), buf.len()) };

    if res == libc::ERANGE {
      if buf.len() > 64 * 1024 {
        // on macOS, if the buf is "too big", ttyname_r can
        // return ERANGE, even though that is supposed to
        // indicate buf is "too small".
        return None;
      }
      buf.resize(buf.len() * 2, 0 as std::ffi::c_char);
      continue;
    }

    return if res == 0 {
      let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
      let osstr = OsStr::from_bytes(cstr.to_bytes());
      Some(PathBuf::from(osstr))
    } else {
      None
    };
  }
}

/// On Big Sur, Cocoa leaks various file descriptors to child processes,
/// so we need to make a pass through the open descriptors beyond just the
/// stdio descriptors and close them all out.
/// This is approximately equivalent to the darwin `posix_spawnattr_setflags`
/// option POSIX_SPAWN_CLOEXEC_DEFAULT which is used as a bit of a cheat
/// on macOS.
/// On Linux, gnome/mutter leak shell extension fds to wezterm too, so we
/// also need to make an effort to clean up the mess.
///
/// This function enumerates the open filedescriptors in the current process
/// and then will forcibly call close(2) on each open fd that is numbered
/// 3 or higher, effectively closing all descriptors except for the stdio
/// streams.
///
/// The implementation of this function relies on `/dev/fd` being available
/// to provide the list of open fds.  Any errors in enumerating or closing
/// the fds are silently ignored.
fn close_random_fds() {
  // FreeBSD, macOS and presumably other BSDish systems have /dev/fd as
  // a directory listing the current fd numbers for the process.
  //
  // On Linux, /dev/fd is a symlink to /proc/self/fd
  if let Ok(dir) = std::fs::read_dir("/proc/self/fd").or_else(|_| std::fs::read_dir("/dev/fd")) {
    let mut fds = vec![];
    for entry in dir {
      if let Some(num) = entry
        .ok()
        .map(|e| e.file_name())
        .and_then(|s| s.into_string().ok())
        .and_then(|n| n.parse::<libc::c_int>().ok())
        && num > 2
      {
        fds.push(num);
      }
    }
    for fd in fds {
      let _ = nix::unistd::close(fd);
    }
  }
}

impl PtyFd {
  fn resize(&self, size: PtySize) -> Result<(), Error> {
    let ws_size = winsize {
      ws_row: size.rows,
      ws_col: size.cols,
      ws_xpixel: size.pixel_width,
      ws_ypixel: size.pixel_height,
    };

    if unsafe {
      libc::ioctl(
        self.0.as_raw_fd(),
        libc::TIOCSWINSZ as _,
        &ws_size as *const _,
      )
    } != 0
    {
      bail!(
        "failed to ioctl(TIOCSWINSZ): {:?}",
        io::Error::last_os_error()
      );
    }

    Ok(())
  }

  fn get_size(&self) -> Result<PtySize, Error> {
    let mut size: winsize = unsafe { mem::zeroed() };
    if unsafe {
      libc::ioctl(
        self.0.as_raw_fd(),
        libc::TIOCGWINSZ as _,
        &mut size as *mut _,
      )
    } != 0
    {
      bail!(
        "failed to ioctl(TIOCGWINSZ): {:?}",
        io::Error::last_os_error()
      );
    }
    Ok(PtySize {
      rows: size.ws_row,
      cols: size.ws_col,
      pixel_width: size.ws_xpixel,
      pixel_height: size.ws_ypixel,
    })
  }

  fn spawn_command(
    &self,
    command: CommandBuilder,
    pre_exec: impl FnOnce(&Path) -> color_eyre::Result<()> + Send + Sync + 'static,
  ) -> color_eyre::Result<Pid> {
    spawn_command_from_pty_fd(Some(self), command, pre_exec)
  }
}

pub fn spawn_command(
  pts: Option<&UnixSlavePty>,
  command: CommandBuilder,
  pre_exec: impl FnOnce(&Path) -> color_eyre::Result<()> + Send + Sync + 'static,
) -> color_eyre::Result<Pid> {
  if let Some(pts) = pts {
    pts.spawn_command(command, pre_exec)
  } else {
    spawn_command_from_pty_fd(None, command, pre_exec)
  }
}

fn spawn_command_from_pty_fd(
  pty: Option<&PtyFd>,
  command: CommandBuilder,
  pre_exec: impl FnOnce(&Path) -> color_eyre::Result<()> + Send + Sync + 'static,
) -> color_eyre::Result<Pid> {
  let configured_umask = command.umask;

  let mut cmd = command.build()?;

  match unsafe { fork()? } {
    nix::unistd::ForkResult::Parent { child } => Ok(child),
    nix::unistd::ForkResult::Child => {
      if let Some(pty) = pty {
        dup2(pty.as_raw_fd(), 0).unwrap();
        dup2(pty.as_raw_fd(), 1).unwrap();
        dup2(pty.as_raw_fd(), 2).unwrap();
      }

      // Clean up a few things before we exec the program
      // Clear out any potentially problematic signal
      // dispositions that we might have inherited
      for signo in &[
        libc::SIGCHLD,
        libc::SIGHUP,
        libc::SIGINT,
        libc::SIGQUIT,
        libc::SIGTERM,
        libc::SIGALRM,
      ] {
        unsafe {
          _ = libc::signal(*signo, libc::SIG_DFL);
        }
      }

      unsafe {
        let empty_set: libc::sigset_t = std::mem::zeroed();
        _ = libc::sigprocmask(libc::SIG_SETMASK, &empty_set, std::ptr::null_mut());
      }

      pre_exec(&cmd.program).unwrap();

      close_random_fds();

      if let Some(mask) = configured_umask {
        _ = unsafe { libc::umask(mask) };
      }

      execv(
        &CString::new(cmd.program.into_os_string().into_vec()).unwrap(),
        &cmd.args,
      )
      .unwrap();
      unreachable!()
    }
  }
}

/// Represents the master end of a pty.
/// The file descriptor will be closed when the Pty is dropped.
pub struct UnixMasterPty {
  fd: PtyFd,
  took_writer: RefCell<bool>,
  tty_name: Option<PathBuf>,
}

/// Represents the slave end of a pty.
/// The file descriptor will be closed when the Pty is dropped.
pub struct UnixSlavePty {
  pub fd: PtyFd,
}

impl UnixSlavePty {
  pub fn spawn_command(
    &self,
    command: CommandBuilder,
    pre_exec: impl FnOnce(&Path) -> color_eyre::Result<()> + Send + Sync + 'static,
  ) -> color_eyre::Result<Pid> {
    self.fd.spawn_command(command, pre_exec)
  }
}

/// Helper function to set the close-on-exec flag for a raw descriptor
fn cloexec(fd: RawFd) -> Result<(), Error> {
  let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
  if flags == -1 {
    bail!(
      "fcntl to read flags failed: {:?}",
      io::Error::last_os_error()
    );
  }
  let result = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
  if result == -1 {
    bail!(
      "fcntl to set CLOEXEC failed: {:?}",
      io::Error::last_os_error()
    );
  }
  Ok(())
}

impl MasterPty for UnixMasterPty {
  fn resize(&self, size: PtySize) -> Result<(), Error> {
    self.fd.resize(size)
  }

  fn get_size(&self) -> Result<PtySize, Error> {
    self.fd.get_size()
  }

  fn try_clone_reader(&self) -> Result<Box<dyn Read + Send>, Error> {
    let fd = PtyFd(self.fd.try_clone()?);
    Ok(Box::new(fd))
  }

  fn take_writer(&self) -> Result<Box<dyn Write + Send>, Error> {
    if *self.took_writer.borrow() {
      bail!("cannot take writer more than once");
    }
    *self.took_writer.borrow_mut() = true;
    let fd = PtyFd(self.fd.try_clone()?);
    Ok(Box::new(UnixMasterWriter { fd }))
  }

  fn as_raw_fd(&self) -> Option<RawFd> {
    Some(self.fd.0.as_raw_fd())
  }

  fn tty_name(&self) -> Option<PathBuf> {
    self.tty_name.clone()
  }

  fn process_group_leader(&self) -> Option<libc::pid_t> {
    match unsafe { libc::tcgetpgrp(self.fd.0.as_raw_fd()) } {
      pid if pid > 0 => Some(pid),
      _ => None,
    }
  }

  fn get_termios(&self) -> Option<nix::sys::termios::Termios> {
    nix::sys::termios::tcgetattr(unsafe { File::from_raw_fd(self.fd.0.as_raw_fd()) }).ok()
  }
}

/// Represents the master end of a pty.
/// EOT will be sent, and then the file descriptor will be closed when
/// the Pty is dropped.
struct UnixMasterWriter {
  fd: PtyFd,
}

impl Drop for UnixMasterWriter {
  fn drop(&mut self) {
    let mut t: libc::termios = unsafe { std::mem::MaybeUninit::zeroed().assume_init() };
    if unsafe { libc::tcgetattr(self.fd.0.as_raw_fd(), &mut t) } == 0 {
      // EOF is only interpreted after a newline, so if it is set,
      // we send a newline followed by EOF.
      let eot = t.c_cc[libc::VEOF];
      if eot != 0 {
        let _ = self.fd.0.write_all(&[b'\n', eot]);
      }
    }
  }
}

impl Write for UnixMasterWriter {
  fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
    self.fd.write(buf)
  }
  fn flush(&mut self) -> Result<(), io::Error> {
    self.fd.flush()
  }
}
