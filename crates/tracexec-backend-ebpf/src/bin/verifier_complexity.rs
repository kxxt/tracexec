use std::{
  env,
  io,
  mem::MaybeUninit,
  os::unix::fs::MetadataExt,
};

use color_eyre::{
  Result,
  eyre::{
    Context,
    bail,
    eyre,
  },
};
use libbpf_rs::{
  AsRawLibbpf,
  num_possible_cpus,
  skel::{
    OpenSkel,
    SkelBuilder,
  },
};
use libbpf_sys::{
  BPF_F_NO_PREALLOC,
  BPF_F_SLEEPABLE,
};
use serde::Serialize;
#[cfg(target_arch = "x86_64")]
use tracexec_backend_ebpf::probe::should_load_compat_syscall_hooks;
use tracexec_backend_ebpf::{
  bpf::skel::{
    OpenTracexecSystemSkel,
    TracexecSystemSkelBuilder,
  },
  probe::{
    can_i_use_sleepable_fentry,
    kernel_have_ftrace_with_direct_calls,
    kernel_have_syscall_wrappers,
    kernel_rejects_syscall_wrapper_kprobes,
    kernel_supports_sleepable_no_prealloc_hash_maps,
  },
  test_utils::{
    KCONFIG,
    disable_all_programs,
    prepare_execve_fentry_fexit,
    prepare_execve_kprobe_kretprobe,
    prepare_execveat_fentry_fexit,
    prepare_execveat_kprobe_kretprobe,
    prepare_handle_exit_only,
    prepare_trace_fork_only,
  },
};

const BPF_LOG_STATS: u32 = 4;
const DEFAULT_LOG_BYTES: usize = 1024 * 1024;
const COLLECTION: &str = "tracexec_system";

type PrepareFn = for<'obj> fn(&mut OpenTracexecSystemSkel<'obj>) -> Result<()>;
type SkipFn = fn() -> Option<&'static str>;

struct LoadCase {
  name: &'static str,
  prepare: PrepareFn,
  skip: SkipFn,
  skip_invalid_argument: bool,
}

struct VerifierLog {
  program: String,
  buf: Vec<u8>,
}

#[derive(Debug)]
struct VerifierMetrics {
  insns_processed: u64,
  insns_limit: u64,
  max_states_per_insn: u64,
  total_states: u64,
  peak_states: u64,
  mark_read: u64,
  verification_time_microseconds: Option<u64>,
  stack_depth: Option<u64>,
  stack_depths: Vec<u64>,
}

#[derive(Serialize)]
struct ComplexityRecord {
  collection: &'static str,
  build: String,
  load: &'static str,
  program: String,
  arch: &'static str,
  kernel_release: String,
  insns_processed: u64,
  insns_limit: u64,
  max_states_per_insn: u64,
  total_states: u64,
  peak_states: u64,
  mark_read: u64,
  #[serde(skip_serializing_if = "Option::is_none")]
  verification_time_microseconds: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  stack_depth: Option<u64>,
  stack_depths: Vec<u64>,
}

fn main() -> Result<()> {
  color_eyre::install()?;
  bump_memlock_rlimit()?;

  let build = env::var("TRACEXEC_VERIFIER_BUILD").unwrap_or_else(|_| "default".to_string());
  let kernel_release = std::fs::read_to_string("/proc/sys/kernel/osrelease")
    .unwrap_or_else(|_| "unknown".to_string())
    .trim()
    .to_string();
  let log_bytes = env::var("TRACEXEC_VERIFIER_LOG_BYTES")
    .ok()
    .map(|v| v.parse::<usize>())
    .transpose()
    .wrap_err("invalid TRACEXEC_VERIFIER_LOG_BYTES")?
    .unwrap_or(DEFAULT_LOG_BYTES);

  let mut records = Vec::new();
  for case in load_cases() {
    records.extend(run_case(&case, &build, &kernel_release, log_bytes)?);
  }
  if records.is_empty() {
    bail!("no verifier complexity records were collected");
  }

  records.sort_by(|a, b| (a.load, a.program.as_str()).cmp(&(b.load, b.program.as_str())));
  serde_json::to_writer_pretty(io::stdout(), &records)?;
  println!();
  Ok(())
}

fn bump_memlock_rlimit() -> Result<()> {
  let rlimit = nix::libc::rlimit {
    rlim_cur: 128 << 20,
    rlim_max: 128 << 20,
  };

  if unsafe { nix::libc::setrlimit(nix::libc::RLIMIT_MEMLOCK, &rlimit) } != 0 {
    bail!("failed to increase RLIMIT_MEMLOCK");
  }
  Ok(())
}

fn load_cases() -> Vec<LoadCase> {
  let mut cases = vec![
    LoadCase {
      name: "production",
      prepare: prepare_production,
      skip: never_skip,
      skip_invalid_argument: false,
    },
    LoadCase {
      name: "trace-fork",
      prepare: prepare_trace_fork,
      skip: never_skip,
      skip_invalid_argument: false,
    },
    LoadCase {
      name: "process-free",
      prepare: prepare_process_free,
      skip: never_skip,
      skip_invalid_argument: false,
    },
    LoadCase {
      name: "handle-exit",
      prepare: prepare_handle_exit,
      skip: never_skip,
      skip_invalid_argument: false,
    },
    LoadCase {
      name: "execve-kprobe",
      prepare: prepare_execve_kprobe,
      skip: skip_syscall_wrapper_kprobe,
      skip_invalid_argument: false,
    },
    LoadCase {
      name: "execveat-kprobe",
      prepare: prepare_execveat_kprobe,
      skip: skip_syscall_wrapper_kprobe,
      skip_invalid_argument: false,
    },
    LoadCase {
      name: "execve-fentry",
      prepare: prepare_execve_fentry,
      skip: skip_fentry,
      skip_invalid_argument: true,
    },
    LoadCase {
      name: "execveat-fentry",
      prepare: prepare_execveat_fentry,
      skip: skip_fentry,
      skip_invalid_argument: true,
    },
  ];

  #[cfg(target_arch = "x86_64")]
  {
    cases.extend([
      LoadCase {
        name: "compat-execve-fentry",
        prepare: prepare_compat_execve_fentry,
        skip: skip_compat_fentry,
        skip_invalid_argument: true,
      },
      LoadCase {
        name: "compat-execveat-fentry",
        prepare: prepare_compat_execveat_fentry,
        skip: skip_compat_fentry,
        skip_invalid_argument: true,
      },
    ]);
  }

  cases
}

fn never_skip() -> Option<&'static str> {
  None
}

fn skip_fentry() -> Option<&'static str> {
  (!kernel_have_ftrace_with_direct_calls(KCONFIG.as_ref(), None))
    .then_some("missing CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS")
}

#[cfg(target_arch = "x86_64")]
fn skip_compat_fentry() -> Option<&'static str> {
  if !should_load_compat_syscall_hooks(KCONFIG.as_ref()) {
    Some("missing CONFIG_IA32_EMULATION")
  } else {
    skip_fentry()
  }
}

fn skip_syscall_wrapper_kprobe() -> Option<&'static str> {
  kernel_rejects_syscall_wrapper_kprobes(KCONFIG.as_ref())
    .then_some("kernel rejects syscall-wrapper kprobes")
}

fn run_case(
  case: &LoadCase,
  build: &str,
  kernel_release: &str,
  log_bytes: usize,
) -> Result<Vec<ComplexityRecord>> {
  if let Some(reason) = (case.skip)() {
    eprintln!("skipping {}: {reason}", case.name);
    return Ok(Vec::new());
  }

  let mut obj = MaybeUninit::uninit();
  let builder = TracexecSystemSkelBuilder::default();
  let mut open_skel = builder.open(&mut obj)?;
  (case.prepare)(&mut open_skel)
    .with_context(|| format!("failed to prepare load case {}", case.name))?;
  apply_common_map_setup(&mut open_skel)?;

  let logs = install_verifier_logs(&mut open_skel, log_bytes)
    .with_context(|| format!("failed to install verifier logs for {}", case.name))?;

  let _skel = match open_skel.load() {
    Ok(skel) => skel,
    Err(err)
      if case.skip_invalid_argument
        && format!("{err:?}").contains("Invalid argument (os error 22)") =>
    {
      eprintln!(
        "skipping {}: kernel rejected this program flavor: {err}",
        case.name
      );
      return Ok(Vec::new());
    }
    Err(err) => return Err(err).with_context(|| format!("failed to load {}", case.name)),
  };

  logs
    .into_iter()
    .map(|log| {
      let text = verifier_log_text(&log.buf);
      let metrics = parse_verifier_metrics(&text).ok_or_else(|| {
        eyre!(
          "verifier metrics not found for {} in load case {}; log was:\n{}",
          log.program,
          case.name,
          text
        )
      })?;
      Ok(ComplexityRecord {
        collection: COLLECTION,
        build: build.to_string(),
        load: case.name,
        program: log.program,
        arch: env::consts::ARCH,
        kernel_release: kernel_release.to_string(),
        insns_processed: metrics.insns_processed,
        insns_limit: metrics.insns_limit,
        max_states_per_insn: metrics.max_states_per_insn,
        total_states: metrics.total_states,
        peak_states: metrics.peak_states,
        mark_read: metrics.mark_read,
        verification_time_microseconds: metrics.verification_time_microseconds,
        stack_depth: metrics.stack_depth,
        stack_depths: metrics.stack_depths,
      })
    })
    .collect()
}

fn install_verifier_logs(
  open_skel: &mut OpenTracexecSystemSkel<'_>,
  log_bytes: usize,
) -> Result<Vec<VerifierLog>> {
  if log_bytes == 0 {
    bail!("verifier log buffer size must be greater than zero");
  }

  let mut logs = Vec::new();
  for mut prog in open_skel.open_object_mut().progs_mut() {
    if !prog.autoload() {
      continue;
    }
    let program = prog.name().to_string_lossy().into_owned();
    prog.set_log_level(BPF_LOG_STATS);
    let mut buf = vec![0_u8; log_bytes];
    let ret = unsafe {
      libbpf_sys::bpf_program__set_log_buf(
        prog.as_libbpf_object().as_ptr(),
        buf.as_mut_ptr().cast(),
        buf
          .len()
          .try_into()
          .map_err(|_| eyre!("verifier log buffer is too large"))?,
      )
    };
    if ret != 0 {
      bail!(
        "bpf_program__set_log_buf failed for {program}: {}",
        io::Error::from_raw_os_error(-ret)
      );
    }
    logs.push(VerifierLog { program, buf });
  }

  if logs.is_empty() {
    bail!("load case did not enable any BPF programs");
  }
  Ok(logs)
}

fn verifier_log_text(buf: &[u8]) -> String {
  let len = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
  String::from_utf8_lossy(&buf[..len]).into_owned()
}

fn parse_verifier_metrics(log: &str) -> Option<VerifierMetrics> {
  let mut verification_time_microseconds = None;
  let mut stack_depth = None;
  let mut stack_depths = Vec::new();
  let mut processed = None;

  for line in log.lines().map(str::trim) {
    if let Some(value) = line
      .strip_prefix("verification time ")
      .and_then(|s| s.strip_suffix(" usec"))
    {
      verification_time_microseconds = value.parse().ok();
    } else if let Some(value) = line.strip_prefix("stack depth ") {
      if let Some((depths, explicit_max)) = parse_stack_depth_line(value) {
        stack_depth = explicit_max;
        stack_depths = depths;
      }
    } else if line.starts_with("processed ") {
      processed = Some(parse_processed_line(line)?);
    }
  }

  let (insns_processed, insns_limit, max_states_per_insn, total_states, peak_states, mark_read) =
    processed?;

  Some(VerifierMetrics {
    insns_processed,
    insns_limit,
    max_states_per_insn,
    total_states,
    peak_states,
    mark_read,
    verification_time_microseconds,
    stack_depth,
    stack_depths,
  })
}

fn parse_stack_depth_line(line: &str) -> Option<(Vec<u64>, Option<u64>)> {
  let mut parts = line.split_whitespace();
  let depth_list = parts.next()?;
  let depths = depth_list
    .split('+')
    .filter(|part| !part.is_empty())
    .map(str::parse)
    .collect::<Result<Vec<_>, _>>()
    .ok()?;
  if depths.is_empty() {
    return None;
  }

  let mut explicit_max = None;
  while let Some(part) = parts.next() {
    if part == "max" {
      explicit_max = parts.next().and_then(|value| value.parse().ok());
    }
  }

  Some((depths, explicit_max))
}

fn parse_processed_line(line: &str) -> Option<(u64, u64, u64, u64, u64, u64)> {
  let parts: Vec<_> = line.split_whitespace().collect();
  if parts.len() < 13
    || parts[0] != "processed"
    || parts[2] != "insns"
    || parts[3] != "(limit"
    || parts[5] != "max_states_per_insn"
    || parts[7] != "total_states"
    || parts[9] != "peak_states"
    || parts[11] != "mark_read"
  {
    return None;
  }

  Some((
    parts[1].parse().ok()?,
    parts[4].trim_end_matches(')').parse().ok()?,
    parts[6].parse().ok()?,
    parts[8].parse().ok()?,
    parts[10].parse().ok()?,
    parts[12].parse().ok()?,
  ))
}

fn apply_common_map_setup(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  if kernel_supports_sleepable_no_prealloc_hash_maps() {
    open_skel
      .maps
      .tracee_closure
      .set_map_flags(BPF_F_NO_PREALLOC)?;
  }
  Ok(())
}

fn prepare_production(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let rodata = open_skel
    .maps
    .rodata_data
    .as_deref_mut()
    .ok_or_else(|| eyre!("missing rodata map"))?;
  rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  rodata.tracexec_config.tracee_pid = 0;
  rodata.tracexec_config.tracee_pidns_inum = std::fs::metadata("/proc/self/ns/pid")?.ino() as u32;

  #[cfg(target_arch = "x86_64")]
  if !should_load_compat_syscall_hooks(KCONFIG.as_ref()) {
    open_skel.progs.compat_sys_execve.set_autoload(false);
    open_skel.progs.compat_sys_execveat.set_autoload(false);
    open_skel.progs.compat_sys_exit_execve.set_autoload(false);
    open_skel.progs.compat_sys_exit_execveat.set_autoload(false);
  }

  if !kernel_have_ftrace_with_direct_calls(KCONFIG.as_ref(), None) {
    open_skel.progs.sys_execve_fentry.set_autoload(false);
    open_skel.progs.sys_execveat_fentry.set_autoload(false);
    open_skel.progs.sys_exit_execve_fexit.set_autoload(false);
    open_skel.progs.sys_exit_execveat_fexit.set_autoload(false);
  } else {
    open_skel.progs.sys_execve_kprobe.set_autoload(false);
    open_skel.progs.sys_execveat_kprobe.set_autoload(false);
    open_skel
      .progs
      .sys_exit_execve_kretprobe
      .set_autoload(false);
    open_skel
      .progs
      .sys_exit_execveat_kretprobe
      .set_autoload(false);

    if can_i_use_sleepable_fentry(KCONFIG.as_ref(), None) {
      rodata.tracexec_config.sleepable = MaybeUninit::new(true);
      open_skel.progs.sys_execve_fentry.set_flags(BPF_F_SLEEPABLE);
      open_skel
        .progs
        .sys_execveat_fentry
        .set_flags(BPF_F_SLEEPABLE);
      #[cfg(target_arch = "x86_64")]
      {
        open_skel.progs.compat_sys_execve.set_flags(BPF_F_SLEEPABLE);
        open_skel
          .progs
          .compat_sys_execveat
          .set_flags(BPF_F_SLEEPABLE);
      }
    }
  }

  if !kernel_have_syscall_wrappers(KCONFIG.as_ref()) {
    open_skel.progs.sys_execve_kprobe.set_autoattach(false);
    open_skel.progs.sys_execveat_kprobe.set_autoattach(false);
    open_skel
      .progs
      .sys_exit_execve_kretprobe
      .set_autoattach(false);
    open_skel
      .progs
      .sys_exit_execveat_kretprobe
      .set_autoattach(false);
  }

  let cache_size: u32 = (2 * num_possible_cpus()?)
    .try_into()
    .map_err(|_| eyre!("too many possible CPUs"))?;
  open_skel.maps.cache.set_max_entries(cache_size)?;

  Ok(())
}

fn prepare_trace_fork(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let _ = prepare_trace_fork_only(open_skel);
  Ok(())
}

fn prepare_process_free(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  disable_all_programs(open_skel);
  open_skel.progs.handle_process_free.set_autoload(true);
  open_skel.progs.handle_process_free.set_autoattach(true);
  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(true);
  }
  Ok(())
}

fn prepare_handle_exit(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let _ = prepare_handle_exit_only(open_skel);
  Ok(())
}

fn prepare_execve_kprobe(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let _ = prepare_execve_kprobe_kretprobe(open_skel);
  Ok(())
}

fn prepare_execveat_kprobe(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let _ = prepare_execveat_kprobe_kretprobe(open_skel);
  Ok(())
}

fn prepare_execve_fentry(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let _ = prepare_execve_fentry_fexit(open_skel);
  Ok(())
}

fn prepare_execveat_fentry(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  let _ = prepare_execveat_fentry_fexit(open_skel);
  Ok(())
}

#[cfg(target_arch = "x86_64")]
fn prepare_compat_execve_fentry(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  use tracexec_backend_ebpf::test_utils::prepare_compat_execve;

  let _ = prepare_compat_execve(open_skel);
  Ok(())
}

#[cfg(target_arch = "x86_64")]
fn prepare_compat_execveat_fentry(open_skel: &mut OpenTracexecSystemSkel<'_>) -> Result<()> {
  use tracexec_backend_ebpf::test_utils::prepare_compat_execveat;

  let _ = prepare_compat_execveat(open_skel);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_stack_depth_with_explicit_max() {
    let metrics = parse_verifier_metrics(
      "verification time 955 usec\n\
       stack depth 24+16 max 48\n\
       insns processed 96+7\n\
       processed 103 insns (limit 1000000) max_states_per_insn 0 total_states 8 peak_states 8 mark_read 0\n",
    )
    .expect("metrics should parse");

    assert_eq!(metrics.verification_time_microseconds, Some(955));
    assert_eq!(metrics.stack_depth, Some(48));
    assert_eq!(metrics.stack_depths, vec![24, 16]);
    assert_eq!(metrics.insns_processed, 103);
    assert_eq!(metrics.insns_limit, 1_000_000);
    assert_eq!(metrics.total_states, 8);
    assert_eq!(metrics.peak_states, 8);
  }

  #[test]
  fn leaves_stack_depth_unset_without_explicit_max() {
    let metrics = parse_verifier_metrics(
      "verification time 10 usec\n\
       stack depth 24+16\n\
       processed 103 insns (limit 1000000) max_states_per_insn 0 total_states 8 peak_states 8 mark_read 0\n",
    )
    .expect("metrics should parse");

    assert_eq!(metrics.stack_depth, None);
    assert_eq!(metrics.stack_depths, vec![24, 16]);
  }

  #[test]
  fn unknown_stack_depth_format_does_not_hide_processed_metrics() {
    let metrics = parse_verifier_metrics(
      "stack depth some future format\n\
       processed 103 insns (limit 1000000) max_states_per_insn 0 total_states 8 peak_states 8 mark_read 0\n",
    )
    .expect("processed metrics should still parse");

    assert_eq!(metrics.stack_depth, None);
    assert!(metrics.stack_depths.is_empty());
    assert_eq!(metrics.insns_processed, 103);
  }
}
