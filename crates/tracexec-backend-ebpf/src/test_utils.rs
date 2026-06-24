#![allow(dead_code)]

use std::{
  env,
  mem::MaybeUninit,
  path::PathBuf,
  sync::LazyLock,
};

use libbpf_rs::skel::{
  OpenSkel,
  Skel,
  SkelBuilder,
};
use libbpf_sys::{
  BPF_F_NO_PREALLOC,
  BPF_F_SLEEPABLE,
};
use procfs::ConfigSetting;
use tracing::warn;

use crate::{
  bpf::skel::{
    OpenTracexecSystemSkel,
    TracexecSystemSkel,
    TracexecSystemSkelBuilder,
  },
  probe::{
    kernel_have_syscall_wrappers,
    kernel_supports_sleepable_no_prealloc_hash_maps,
  },
  tracer::{
    AttachSet,
    attach_kprobes_without_syscall_wrappers,
  },
};

pub static KCONFIG: LazyLock<Option<std::collections::HashMap<String, ConfigSetting>>> =
  LazyLock::new(|| {
    procfs::kernel_config()
      .inspect_err(|e| warn!("Failed to get kernel config during test: {e}"))
      .ok()
  });

pub fn find_sh() -> PathBuf {
  env::var_os("PATH")
    .and_then(|paths| {
      env::split_paths(&paths)
        .filter_map(|dir| {
          let full_path = dir.join("sh");
          if full_path.is_file() {
            Some(full_path)
          } else {
            None
          }
        })
        .next()
    })
    .expect("executable `sh` not found")
}

pub fn disable_all_programs(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  for mut prog in open_skel.open_object_mut().progs_mut() {
    prog.set_autoload(false);
    prog.set_autoattach(false);
  }
}

pub fn prepare_handle_exit_only(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  open_skel.progs.handle_exit.set_autoload(true);
  open_skel.progs.handle_exit.set_autoattach(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  None
}

pub fn prepare_trace_fork_only(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  open_skel.progs.trace_fork.set_autoload(true);
  open_skel.progs.trace_fork.set_autoattach(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  None
}

pub fn prepare_execve_kprobe_kretprobe(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  let kernel_have_syscall_wrappers = kernel_have_syscall_wrappers(KCONFIG.as_ref());
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  open_skel.progs.sys_execve_kprobe.set_autoload(true);
  open_skel.progs.sys_exit_execve_kretprobe.set_autoload(true);
  if !kernel_have_syscall_wrappers {
    Some(Box::new(attach_execve_kprobe_without_syscall_wrappers))
  } else {
    open_skel.progs.sys_execve_kprobe.set_autoattach(true);
    open_skel
      .progs
      .sys_exit_execve_kretprobe
      .set_autoattach(true);
    None
  }
}

fn attach_execve_kprobe_without_syscall_wrappers(
  skel: &mut TracexecSystemSkel<'_>,
) -> Result<(), libbpf_rs::Error> {
  attach_kprobes_without_syscall_wrappers(skel, AttachSet::Execve.into())
}

fn attach_execveat_kprobe_without_syscall_wrappers(
  skel: &mut TracexecSystemSkel<'_>,
) -> Result<(), libbpf_rs::Error> {
  attach_kprobes_without_syscall_wrappers(skel, AttachSet::Execveat.into())
}

#[must_use]
pub fn prepare_execve_fentry_fexit(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  open_skel.progs.sys_execve_fentry.set_autoload(true);
  open_skel.progs.sys_execve_fentry.set_autoattach(true);
  open_skel.progs.sys_exit_execve_fexit.set_autoload(true);
  open_skel.progs.sys_exit_execve_fexit.set_autoattach(true);
  open_skel.progs.sys_execve_fentry.set_flags(BPF_F_SLEEPABLE);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  None
}

#[must_use]
pub fn prepare_execveat_kprobe_kretprobe(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  let kernel_have_syscall_wrappers = kernel_have_syscall_wrappers(KCONFIG.as_ref());
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  open_skel.progs.sys_execveat_kprobe.set_autoload(true);
  open_skel
    .progs
    .sys_exit_execveat_kretprobe
    .set_autoload(true);
  if !kernel_have_syscall_wrappers {
    Some(Box::new(attach_execveat_kprobe_without_syscall_wrappers))
  } else {
    open_skel.progs.sys_execveat_kprobe.set_autoattach(true);
    open_skel
      .progs
      .sys_exit_execveat_kretprobe
      .set_autoattach(true);
    None
  }
}

#[must_use]
pub fn prepare_execveat_fentry_fexit(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  open_skel.progs.sys_execveat_fentry.set_autoload(true);
  open_skel.progs.sys_execveat_fentry.set_autoattach(true);
  open_skel.progs.sys_exit_execveat_fexit.set_autoload(true);
  open_skel.progs.sys_exit_execveat_fexit.set_autoattach(true);
  open_skel
    .progs
    .sys_execveat_fentry
    .set_flags(BPF_F_SLEEPABLE);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  None
}

#[cfg(target_arch = "x86_64")]
#[must_use]
pub fn prepare_compat_execve(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  open_skel.progs.compat_sys_execve.set_autoload(true);
  open_skel.progs.compat_sys_execve.set_autoattach(true);
  open_skel.progs.compat_sys_exit_execve.set_autoload(true);
  open_skel.progs.compat_sys_exit_execve.set_autoattach(true);
  open_skel.progs.compat_sys_execve.set_flags(BPF_F_SLEEPABLE);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  None
}

#[cfg(target_arch = "x86_64")]
pub fn prepare_compat_execveat(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
) -> Option<Box<LoadedSkelCallback>> {
  disable_all_programs(open_skel);
  open_skel.progs.compat_sys_execveat.set_autoload(true);
  open_skel.progs.compat_sys_execveat.set_autoattach(true);
  open_skel.progs.compat_sys_exit_execveat.set_autoload(true);
  open_skel
    .progs
    .compat_sys_exit_execveat
    .set_autoattach(true);
  open_skel
    .progs
    .compat_sys_execveat
    .set_flags(BPF_F_SLEEPABLE);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
  None
}

pub type LoadedSkelCallback = dyn FnOnce(&mut TracexecSystemSkel) -> Result<(), libbpf_rs::Error>;

pub fn with_skel<T>(
  #[allow(unused)] test_name: &str,
  prepare: impl for<'obj> FnOnce(&mut OpenTracexecSystemSkel<'obj>) -> Option<Box<LoadedSkelCallback>>,
  f: impl for<'obj> FnOnce(&mut TracexecSystemSkel<'obj>) -> color_eyre::Result<T>,
) -> color_eyre::Result<T> {
  let mut obj = MaybeUninit::uninit();
  let builder = TracexecSystemSkelBuilder::default();
  let mut open_skel = builder.open(&mut obj)?;
  let callback = prepare(&mut open_skel);
  if kernel_supports_sleepable_no_prealloc_hash_maps() {
    open_skel
      .maps
      .tracee_closure
      .set_map_flags(BPF_F_NO_PREALLOC)?;
  }
  let mut skel = open_skel.load()?;
  skel.attach()?;
  callback.map(|cb| cb(&mut skel)).transpose()?;
  let result = f(&mut skel);

  #[cfg(feature = "bpfcov")]
  if let Some(outdir) = env::var_os("TRACEXEC_BPFCOV_OUTDIR") {
    let test_dir = PathBuf::from(outdir).join(test_name);
    std::fs::create_dir_all(&test_dir).expect("failed to create test output directory");
    let profraw = test_dir.join("tracexec.profraw");
    crate::coverage::write_coverage(skel.object(), &profraw)
      .expect("failed to write coverage data");
    crate::coverage::export_lcov(&profraw, &test_dir).expect("failed to export lcov data");
  }

  result
}

/// Get the name of the enclosing function.
#[macro_export]
macro_rules! function_name {
  () => {{
    fn f() {}
    fn type_name_of<T>(_: T) -> &'static str {
      std::any::type_name::<T>()
    }
    let name = type_name_of(f);
    name.rsplit("::").nth(1).unwrap_or(name)
  }};
}
