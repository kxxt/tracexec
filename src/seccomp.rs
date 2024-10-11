use libseccomp::{ScmpAction, ScmpFilterContext};

pub fn create_seccomp_filters() -> color_eyre::Result<ScmpFilterContext> {
  let mut filter = ScmpFilterContext::new_filter(ScmpAction::Allow)?;
  filter.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execve as i32)?;
  filter.add_rule(ScmpAction::Trace(0), nix::libc::SYS_execveat as i32)?;
  Ok(filter)
}
