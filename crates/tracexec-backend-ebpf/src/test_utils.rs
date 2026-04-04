#![allow(dead_code)]

use std::{
  env,
  mem::MaybeUninit,
  path::PathBuf,
};

use libbpf_rs::skel::{
  OpenSkel,
  Skel,
  SkelBuilder,
};

use crate::bpf::skel::{
  OpenTracexecSystemSkel,
  TracexecSystemSkel,
  TracexecSystemSkelBuilder,
};

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
  }
}

pub fn prepare_handle_exit_only(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.handle_exit.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

pub fn prepare_trace_fork_only(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.trace_fork.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

pub fn prepare_execve_kprobe_kretprobe(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.sys_execve_kprobe.set_autoload(true);
  open_skel.progs.sys_exit_execve_kretprobe.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

pub fn prepare_execve_fentry_fexit(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.sys_execve_fentry.set_autoload(true);
  open_skel.progs.sys_exit_execve_fexit.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

pub fn prepare_execveat_kprobe_kretprobe(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.sys_execveat_kprobe.set_autoload(true);
  open_skel
    .progs
    .sys_exit_execveat_kretprobe
    .set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

pub fn prepare_execveat_fentry_fexit(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.sys_execveat_fentry.set_autoload(true);
  open_skel.progs.sys_exit_execveat_fexit.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

#[cfg(target_arch = "x86_64")]
pub fn prepare_compat_execve(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.compat_sys_execve.set_autoload(true);
  open_skel.progs.compat_sys_exit_execve.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

#[cfg(target_arch = "x86_64")]
pub fn prepare_compat_execveat(open_skel: &mut OpenTracexecSystemSkel<'_>) {
  disable_all_programs(open_skel);
  open_skel.progs.compat_sys_execveat.set_autoload(true);
  open_skel.progs.compat_sys_exit_execveat.set_autoload(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }
}

pub fn with_skel<T>(
  #[allow(unused)] test_name: &str,
  prepare: impl for<'obj> FnOnce(&mut OpenTracexecSystemSkel<'obj>),
  f: impl for<'obj> FnOnce(&mut TracexecSystemSkel<'obj>) -> color_eyre::Result<T>,
) -> color_eyre::Result<T> {
  let mut obj = MaybeUninit::uninit();
  let builder = TracexecSystemSkelBuilder::default();
  let mut open_skel = builder.open(&mut obj)?;
  prepare(&mut open_skel);
  let mut skel = open_skel.load()?;
  skel.attach()?;
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
