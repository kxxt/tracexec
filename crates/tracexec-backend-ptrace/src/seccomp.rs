use libseccomp::{ScmpAction, ScmpArch, ScmpFilterContext};

pub fn load_seccomp_filters() -> color_eyre::Result<()> {
  libseccomp::reset_global_state()?;
  let mut filter = ScmpFilterContext::new(ScmpAction::Allow)?;
  filter.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execve as i32)?;
  filter.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execveat as i32)?;
  if cfg!(target_arch = "x86_64") {
    let mut filter32 = ScmpFilterContext::new(ScmpAction::Allow)?;
    filter32.remove_arch(ScmpArch::native())?;
    filter32.add_arch(ScmpArch::X86)?;
    // libseccomp translates the syscall number for us.
    filter32.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execve as i32)?;
    filter32.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execveat as i32)?;
    filter.merge(filter32)?;
  }
  filter.load()?;
  Ok(())
}

#[cfg(test)]
mod test {
  use nix::errno::Errno;
  use nix::fcntl::AtFlags;
  use nix::fcntl::OFlag;
  use nix::sys::stat::Mode;
  use nix::unistd::execve;
  use nix::unistd::execveat;
  use rusty_fork::rusty_fork_test;

  use crate::seccomp::load_seccomp_filters;

  rusty_fork_test! {
    #[test]
    fn seccomp_filter_loads() {
      // Load the filter
      load_seccomp_filters().expect("Failed to load seccomp filter");
      // Check if the syscall hits the filter.
      // This should return ENOSYS as we don't attach a tracer to this subprocess
      assert_eq!(execve(c"/", &[c""], &[c""]), Err(Errno::ENOSYS));
      let fd = nix::fcntl::open("/", OFlag::O_PATH, Mode::empty()).unwrap();
      assert_eq!(execveat(&fd, c"/", &[c""], &[c""], AtFlags::empty()), Err(Errno::ENOSYS));
    }
  }
}
