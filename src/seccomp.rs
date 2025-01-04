use libseccomp::{ScmpAction, ScmpArch, ScmpFilterContext};

pub fn load_seccomp_filters() -> color_eyre::Result<()> {
  libseccomp::reset_global_state()?;
  let mut filter = ScmpFilterContext::new_filter(ScmpAction::Allow)?;
  filter.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execve as i32)?;
  filter.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execveat as i32)?;
  if cfg!(target_arch = "x86_64") {
    let mut filter32 = ScmpFilterContext::new_filter(ScmpAction::Allow)?;
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
