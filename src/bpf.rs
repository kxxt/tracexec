use std::{
  mem::MaybeUninit,
  process,
  sync::{
    Arc,
    atomic::Ordering,
  },
};

use color_eyre::{
  Section,
  eyre::eyre,
};
use enumflags2::BitFlag;
use futures::stream::StreamExt;
use nix::unistd::User;
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;
use tokio::{
  sync::mpsc::{
    self,
  },
  task::spawn_blocking,
};
use tracexec_backend_ebpf::bpf::tracer::BuildEbpfTracer;
use tracexec_core::{
  cli::{
    Cli,
    EbpfCommand,
    args::LogModeArgs,
    options::{
      Color,
      ExportFormat,
    },
  },
  event::TracerEventDetailsKind,
  export::{
    Exporter,
    ExporterMetadata,
  },
  printer::{
    Printer,
    PrinterArgs,
  },
  proc::BaselineInfo,
  pty::{
    PtySize,
    PtySystem,
    native_pty_system,
  },
  tracer::{
    TracerBuilder,
    TracerMode,
  },
};
use tracexec_exporter_json::{
  JsonExporter,
  JsonStreamExporter,
};
use tracexec_exporter_perfetto::PerfettoExporter;
use tracexec_tui::{
  self,
  app::App,
};

pub async fn main(
  command: EbpfCommand,
  user: Option<User>,
  color: Color,
) -> color_eyre::Result<()> {
  let obj = Box::leak(Box::new(MaybeUninit::uninit()));
  match command {
    EbpfCommand::Log {
      cmd,
      output,
      modifier_args,
      log_args,
    } => {
      let modifier_args = modifier_args.processed();
      let baseline = Arc::new(BaselineInfo::new()?);
      let output = Cli::get_output(output, color)?;
      let printer = Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      );
      let tracer = TracerBuilder::new()
        .mode(TracerMode::Log {
          foreground: log_args.foreground(),
        })
        .filter(TracerEventDetailsKind::empty())
        .printer(printer)
        .baseline(baseline)
        .user(user)
        .modifier(modifier_args)
        .build_ebpf();
      let running_tracer = tracer.spawn(&cmd, obj, Some(output))?;
      running_tracer.run_until_exit();
      Ok(())
    }
    EbpfCommand::Tui {
      cmd,
      modifier_args,
      tracer_event_args,
      tui_args,
    } => {
      let follow_forks = !cmd.is_empty();
      if tui_args.tty && !follow_forks {
        return Err(
          eyre!("--tty is not supported for eBPF system-wide tracing.").with_suggestion(
            || "Did you mean to use follow-fork mode? e.g. tracexec ebpf tui -t -- bash",
          ),
        );
      }
      let modifier_args = modifier_args.processed();
      // Disable owo-colors when running TUI
      owo_colors::control::set_should_colorize(false);
      let (baseline, tracer_mode, pty_master) = if tui_args.tty {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
          rows: 24,
          cols: 80,
          pixel_width: 0,
          pixel_height: 0,
        })?;
        (
          BaselineInfo::with_pts(&pair.slave)?,
          TracerMode::Tui(Some(pair.slave)),
          Some(pair.master),
        )
      } else {
        (BaselineInfo::new()?, TracerMode::Tui(None), None)
      };
      let baseline = Arc::new(baseline);
      let frame_rate = tui_args.frame_rate.unwrap_or(60.);
      let log_args = LogModeArgs {
        show_cmdline: false, // We handle cmdline in TUI
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        diff_env: true,
        ..Default::default()
      };
      let mut app = App::new(
        None,
        &log_args,
        &modifier_args,
        tui_args,
        baseline.clone(),
        pty_master,
      )?;
      app.activate_experiment("eBPF");
      let printer = Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      );
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      // let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = TracerBuilder::new()
        .mode(tracer_mode)
        .printer(printer)
        .baseline(baseline)
        .modifier(modifier_args)
        .filter(tracer_event_args.filter()?)
        .user(user)
        .tracer_tx(tracer_tx)
        .build_ebpf();
      let running_tracer = tracer.spawn(&cmd, obj, None)?;
      let should_exit = running_tracer.should_exit.clone();
      let tracer_thread = spawn_blocking(move || {
        running_tracer.run_until_exit();
      });
      let mut tui = tracexec_tui::Tui::new()?.frame_rate(frame_rate);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit()?;
      tracexec_tui::restore_tui()?;
      if !follow_forks {
        should_exit.store(true, Ordering::Relaxed);
      }
      tracer_thread.await?;
      Ok(())
    }
    EbpfCommand::Collect {
      cmd,
      modifier_args,
      exporter_args,
      format,
      output,
      foreground,
      no_foreground,
    } => {
      let modifier_args = modifier_args.processed();
      let baseline = Arc::new(BaselineInfo::new()?);
      let output = Cli::get_output(output, color)?;
      let log_args = LogModeArgs {
        show_cmdline: false,
        show_argv: true,
        show_interpreter: true,
        more_colors: false,
        less_colors: false,
        diff_env: false,
        foreground,
        no_foreground,
        ..Default::default()
      };
      let printer = Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      );
      let (tx, rx) = mpsc::unbounded_channel();
      let tracer = TracerBuilder::new()
        .mode(TracerMode::Log {
          foreground: log_args.foreground(),
        })
        .modifier(modifier_args)
        .printer(printer)
        .baseline(baseline.clone())
        .tracer_tx(tx)
        .user(user)
        .build_ebpf();
      let running_tracer = tracer.spawn(&cmd, obj, None)?;
      let should_exit = running_tracer.should_exit.clone();
      let tracer_thread = spawn_blocking(move || {
        running_tracer.run_until_exit();
      });
      let meta = ExporterMetadata {
        baseline: baseline.clone(),
        exporter_args,
      };
      let signals = Signals::new([SIGTERM, SIGINT])?;
      tokio::spawn(async move {
        let mut signals = signals;
        while let Some(signal) = signals.next().await {
          match signal {
            SIGTERM | SIGINT => {
              should_exit.store(true, Ordering::Relaxed);
            }
            _ => unreachable!(),
          }
        }
      });
      match format {
        ExportFormat::Json => {
          let exporter = JsonExporter::new(output, meta, rx)?;
          let exit_code = exporter.run().await?;
          tracer_thread.await?;
          process::exit(exit_code);
        }
        ExportFormat::JsonStream => {
          let exporter = JsonStreamExporter::new(output, meta, rx)?;
          let exit_code = exporter.run().await?;
          tracer_thread.await?;
          process::exit(exit_code);
        }
        ExportFormat::Perfetto => {
          let exporter = PerfettoExporter::new(output, meta, rx)?;
          let exit_code = exporter.run().await?;
          tracer_thread.await?;
          process::exit(exit_code);
        }
      }
    }
  }
}
