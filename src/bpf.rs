use std::{
  borrow::Cow,
  ffi::CStr,
  mem::MaybeUninit,
  process,
  sync::{Arc, LazyLock, RwLock, atomic::Ordering},
};

use crate::{
  cache::ArcStr,
  export::{Exporter, ExporterMetadata, JsonExporter, JsonStreamExporter, PerfettoExporter},
  tracer::TracerBuilder,
};
use color_eyre::{Section, eyre::eyre};
use enumflags2::BitFlag;
use nix::unistd::User;
use tokio::{
  sync::mpsc::{self},
  task::spawn_blocking,
};

use crate::{
  cache::StringCache,
  cli::{
    Cli, EbpfCommand,
    args::LogModeArgs,
    options::{Color, ExportFormat},
  },
  event::TracerEventDetailsKind,
  printer::{Printer, PrinterArgs},
  proc::BaselineInfo,
  pty::{PtySize, PtySystem, native_pty_system},
  tracer::TracerMode,
  tui::{self, app::App},
};

#[allow(clippy::use_self)] // remove after https://github.com/libbpf/libbpf-rs/pull/1231 is merged
pub mod skel {
  include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/tracexec_system.skel.rs"
  ));
}

pub mod interface {
  include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bpf/interface.rs"));
}

mod event;
mod process_tracker;
mod tracer;
pub use event::BpfError;

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
      let mut tui = tui::Tui::new()?.frame_rate(frame_rate);
      tui.enter(tracer_rx)?;
      app.run(&mut tui).await?;
      // Now when TUI exits, the tracer thread is still running.
      // options:
      // 1. Wait for the tracer thread to exit.
      // 2. Terminate the root process so that the tracer thread exits.
      // 3. Kill the root process so that the tracer thread exits.
      app.exit()?;
      tui::restore_tui()?;
      if !follow_forks {
        should_exit.store(true, Ordering::Relaxed);
      }
      tracer_thread.await?;
      Ok(())
    }
    EbpfCommand::Collect {
      cmd,
      modifier_args,
      format,
      pretty,
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
      let tracer_thread = spawn_blocking(move || {
        running_tracer.run_until_exit();
      });
      let meta = ExporterMetadata {
        baseline: baseline.clone(),
        pretty,
      };
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

fn utf8_lossy_cow_from_bytes_with_nul(data: &[u8]) -> Cow<'_, str> {
  String::from_utf8_lossy(CStr::from_bytes_until_nul(data).unwrap().to_bytes())
}

fn cached_cow(cow: Cow<str>) -> ArcStr {
  match cow {
    Cow::Borrowed(s) => CACHE.write().unwrap().get_or_insert(s),
    Cow::Owned(s) => CACHE.write().unwrap().get_or_insert_owned(s),
  }
}

static CACHE: LazyLock<RwLock<StringCache>> = LazyLock::new(|| RwLock::new(StringCache::new()));
