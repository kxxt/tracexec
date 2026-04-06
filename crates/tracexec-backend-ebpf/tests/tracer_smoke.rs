use std::{
  env,
  ffi::OsString,
  mem::MaybeUninit,
  sync::{
    Arc,
    atomic::Ordering,
  },
};

use enumflags2::BitFlags;
use rstest::rstest;
use serial_test::file_serial;
use tokio::sync::mpsc::UnboundedSender;
use tracexec_backend_ebpf::{
  test_utils::find_sh,
  tracer::BuildEbpfTracer,
};
use tracexec_core::{
  event::{
    TracerEventDetails,
    TracerEventDetailsKind,
    TracerMessage,
  },
  printer::{
    ColorLevel,
    EnvPrintFormat,
    FdPrintFormat,
    Printer,
    PrinterArgs,
  },
  proc::BaselineInfo,
  tracer::{
    TracerBuilder,
    TracerMode,
  },
};

fn test_printer_args() -> PrinterArgs {
  PrinterArgs {
    trace_comm: false,
    trace_argv: true,
    trace_env: EnvPrintFormat::None,
    trace_fd: FdPrintFormat::None,
    trace_cwd: false,
    print_cmdline: false,
    successful_only: false,
    trace_interpreter: false,
    trace_filename: true,
    decode_errno: false,
    color: ColorLevel::Less,
    stdio_in_cmdline: false,
    fd_in_cmdline: false,
    hide_cloexec_fds: false,
    inline_timestamp_format: None,
  }
}

fn build_tracer(
  filter: BitFlags<TracerEventDetailsKind>,
  tx: Option<UnboundedSender<TracerMessage>>,
) -> color_eyre::Result<tracexec_backend_ebpf::tracer::EbpfTracer> {
  let baseline = Arc::new(BaselineInfo::new()?);
  let printer = Printer::new(test_printer_args(), baseline.clone());
  let mut builder = TracerBuilder::new()
    .printer(printer)
    .baseline(baseline)
    .mode(TracerMode::Log { foreground: false })
    .filter(filter);
  if let Some(tx) = tx {
    builder = builder.tracer_tx(tx);
  }
  Ok(builder.build_ebpf())
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_tracer_spawn_runs_to_exit() -> color_eyre::Result<()> {
  let tracer = build_tracer(BitFlags::all(), None)?;
  let sh = find_sh();
  let cmd: Vec<OsString> = vec![sh.into(), "-c".into(), "true".into()];
  let mut obj = MaybeUninit::uninit();
  let running = tracer.spawn(&cmd, &mut obj, None)?;
  running.run_until_exit();
  assert!(running.should_exit.load(Ordering::Relaxed));
  #[cfg(feature = "bpfcov")]
  running
    .save_coverage_if_enabled(tracexec_backend_ebpf::function_name!())
    .expect("failed to save coverage");
  Ok(())
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_tracer_spawn_emits_tracee_exit() -> color_eyre::Result<()> {
  let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
  let tracer = build_tracer(
    BitFlags::from_flag(TracerEventDetailsKind::TraceeExit),
    Some(tx),
  )?;
  let sh = find_sh();
  let cmd: Vec<OsString> = vec![sh.into(), "-c".into(), "true".into()];
  let mut obj = MaybeUninit::uninit();
  let running = tracer.spawn(&cmd, &mut obj, None)?;
  running.run_until_exit();

  let mut saw_exit = false;
  while let Ok(msg) = rx.try_recv() {
    if let TracerMessage::Event(event) = msg
      && matches!(event.details, TracerEventDetails::TraceeExit { .. })
    {
      saw_exit = true;
      break;
    }
  }
  assert!(saw_exit, "missing TraceeExit event");
  #[cfg(feature = "bpfcov")]
  running
    .save_coverage_if_enabled(tracexec_backend_ebpf::function_name!())
    .expect("failed to save coverage");
  Ok(())
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_tracer_spawn_nosleep_loads() -> color_eyre::Result<()> {
  // SAFETY: this test runs sequentially via file_serial(bpf).
  unsafe { env::set_var("TRACEXEC_NO_SLEEP", "1") };
  struct DropGuard;
  impl Drop for DropGuard {
    fn drop(&mut self) {
      // SAFETY: clean up for subsequent tests.
      unsafe { env::remove_var("TRACEXEC_NO_SLEEP") };
    }
  }
  let _guard = DropGuard;
  let tracer = build_tracer(BitFlags::all(), None)?;
  let sh = find_sh();
  let cmd: Vec<OsString> = vec![sh.into(), "-c".into(), "true".into()];
  let mut obj = MaybeUninit::uninit();
  let running = tracer.spawn(&cmd, &mut obj, None)?;
  running.run_until_exit();
  assert!(running.should_exit.load(Ordering::Relaxed));
  #[cfg(feature = "bpfcov")]
  running
    .save_coverage_if_enabled(tracexec_backend_ebpf::function_name!())
    .expect("failed to save coverage");
  Ok(())
}
