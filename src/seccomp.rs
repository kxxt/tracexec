use seccompiler::{SeccompAction, SeccompFilter, TargetArch};

pub fn create_seccomp_filter() -> SeccompFilter {
  SeccompFilter::new(
    vec![
      (nix::libc::SYS_execve, vec![]),
      (nix::libc::SYS_execveat, vec![]),
    ]
    .into_iter()
    .collect(),
    SeccompAction::Allow,
    SeccompAction::Trace(0),
    #[cfg(target_arch = "x86_64")]
    TargetArch::x86_64,
    #[cfg(target_arch = "aarch64")]
    TargetArch::aarch64,
  )
  .expect("failed to create seccomp filter!")
}
