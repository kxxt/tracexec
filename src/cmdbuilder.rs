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

use color_eyre::eyre::{bail, Context};
use nix::libc;
use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use tracing::warn;

fn get_shell() -> String {
  use nix::unistd::{access, AccessFlags};
  use std::ffi::CStr;
  use std::path::Path;
  use std::str;

  let ent = unsafe { libc::getpwuid(libc::getuid()) };
  if !ent.is_null() {
    let shell = unsafe { CStr::from_ptr((*ent).pw_shell) };
    match shell.to_str().map(str::to_owned) {
      Err(err) => {
        warn!(
          "passwd database shell could not be \
                     represented as utf-8: {err:#}, \
                     falling back to /bin/sh"
        );
      }
      Ok(shell) => {
        if let Err(err) = access(Path::new(&shell), AccessFlags::X_OK) {
          warn!(
            "passwd database shell={shell:?} which is \
                         not executable ({err:#}), falling back to /bin/sh"
          );
        } else {
          return shell;
        }
      }
    }
  }
  "/bin/sh".into()
}

/// `CommandBuilder` is used to prepare a command to be spawned into a pty.
/// The interface is intentionally similar to that of `std::process::Command`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandBuilder {
  args: Vec<OsString>,
  cwd: Option<OsString>,
  pub(crate) umask: Option<libc::mode_t>,
  controlling_tty: bool,
}

impl CommandBuilder {
  /// Create a new builder instance with argv[0] set to the specified
  /// program.
  pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
    Self {
      args: vec![program.as_ref().to_owned()],
      cwd: None,
      umask: None,
      controlling_tty: true,
    }
  }

  /// Create a new builder instance from a pre-built argument vector
  pub fn from_argv(args: Vec<OsString>) -> Self {
    Self {
      args,
      cwd: None,
      umask: None,
      controlling_tty: true,
    }
  }

  /// Set whether we should set the pty as the controlling terminal.
  /// The default is true, which is usually what you want, but you
  /// may need to set this to false if you are crossing container
  /// boundaries (eg: flatpak) to workaround issues like:
  /// <https://github.com/flatpak/flatpak/issues/3697>
  /// <https://github.com/flatpak/flatpak/issues/3285>
  pub fn set_controlling_tty(&mut self, controlling_tty: bool) {
    self.controlling_tty = controlling_tty;
  }

  pub fn get_controlling_tty(&self) -> bool {
    self.controlling_tty
  }

  /// Create a new builder instance that will run some idea of a default
  /// program.  Such a builder will panic if `arg` is called on it.
  pub fn new_default_prog() -> Self {
    Self {
      args: vec![],
      cwd: None,
      umask: None,
      controlling_tty: true,
    }
  }

  /// Returns true if this builder was created via `new_default_prog`
  pub fn is_default_prog(&self) -> bool {
    self.args.is_empty()
  }

  /// Append an argument to the current command line.
  /// Will panic if called on a builder created via `new_default_prog`.
  pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) {
    if self.is_default_prog() {
      panic!("attempted to add args to a default_prog builder");
    }
    self.args.push(arg.as_ref().to_owned());
  }

  /// Append a sequence of arguments to the current command line
  pub fn args<I, S>(&mut self, args: I)
  where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
  {
    for arg in args {
      self.arg(arg);
    }
  }

  pub fn get_argv(&self) -> &Vec<OsString> {
    &self.args
  }

  pub fn get_argv_mut(&mut self) -> &mut Vec<OsString> {
    &mut self.args
  }

  pub fn cwd<D>(&mut self, dir: D)
  where
    D: AsRef<OsStr>,
  {
    self.cwd = Some(dir.as_ref().to_owned());
  }

  pub fn clear_cwd(&mut self) {
    self.cwd.take();
  }

  pub fn get_cwd(&self) -> Option<&OsString> {
    self.cwd.as_ref()
  }
}

impl CommandBuilder {
  pub fn umask(&mut self, mask: Option<libc::mode_t>) {
    self.umask = mask;
  }

  fn resolve_path(&self) -> Option<OsString> {
    env::var_os("PATH")
  }

  fn search_path(&self, exe: &OsStr, cwd: &OsStr) -> color_eyre::Result<OsString> {
    use nix::unistd::{access, AccessFlags};
    use std::path::Path;

    let exe_path: &Path = exe.as_ref();
    if exe_path.is_relative() {
      let cwd: &Path = cwd.as_ref();
      let abs_path = cwd.join(exe_path);
      if abs_path.exists() {
        return Ok(abs_path.into_os_string());
      }

      if let Some(path) = self.resolve_path() {
        for path in std::env::split_paths(&path) {
          let candidate = path.join(exe);
          if access(&candidate, AccessFlags::X_OK).is_ok() {
            return Ok(candidate.into_os_string());
          }
        }
      }
      bail!(
        "Unable to spawn {} because it doesn't exist on the filesystem \
                and was not found in PATH",
        exe_path.display()
      );
    } else {
      if let Err(err) = access(exe_path, AccessFlags::X_OK) {
        bail!(
          "Unable to spawn {} because it doesn't exist on the filesystem \
                    or is not executable ({err:#})",
          exe_path.display()
        );
      }

      Ok(exe.to_owned())
    }
  }

  /// Convert the CommandBuilder to a `std::process::Command` instance.
  pub(crate) fn as_command(&self) -> color_eyre::Result<std::process::Command> {
    use std::os::unix::process::CommandExt;
    let cwd = env::current_dir()?;
    let dir = if let Some(dir) = self.cwd.as_deref() {
      dir
    } else {
      cwd.as_os_str()
    };
    let resolved = self.search_path(&self.args[0], dir)?;
    tracing::trace!("resolved path to {:?}", resolved);
    let mut cmd = std::process::Command::new(&resolved);
    cmd.arg0(&self.args[0]);
    cmd.args(&self.args[1..]);

    cmd.current_dir(dir);

    Ok(cmd)
  }
}
