use std::{
  borrow::Cow,
  ffi::CStr,
  mem::MaybeUninit,
  process,
  sync::{
    atomic::Ordering,
    Arc, LazyLock, RwLock,
  },
};

use crate::cache::ArcStr;
use color_eyre::{eyre::eyre, Section};
use enumflags2::BitFlag;
use nix::unistd::User;
use tokio::{
  sync::mpsc::{self},
  task::spawn_blocking,
};
use tracer::EbpfTracer;

use crate::{
  cache::StringCache,
  cli::{
    args::LogModeArgs,
    options::{Color, ExportFormat},
    Cli, EbpfCommand,
  },
  event::{
    TracerEvent, TracerEventDetails,
    TracerEventDetailsKind, TracerMessage,
  },
  export::{self, JsonExecEvent, JsonMetaData},
  printer::{Printer, PrinterArgs},
  proc::BaselineInfo,
  pty::{native_pty_system, PtySize, PtySystem},
  serialize_json_to_output,
  tracer::TracerMode,
  tui::{self, app::App},
};

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

pub async fn main(command: EbpfCommand, user: Option<User>, color: Color) -> color_eyre::Result<()> {
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
      let printer = Arc::new(Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      ));
      let tracer = EbpfTracer::builder()
        .mode(TracerMode::Log {
          foreground: log_args.foreground(),
        })
        .filter(TracerEventDetailsKind::empty())
        .printer(printer)
        .baseline(baseline)
        .user(user)
        .modifier(modifier_args)
        .build();
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
          eyre!("--tty is not supported for eBPF system-wide tracing.").with_suggestion(|| {
            "Did you mean to use follow-fork mode? e.g. tracexec ebpf tui -t -- bash"
          }),
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
      let printer = Arc::new(Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      ));
      let (tracer_tx, tracer_rx) = mpsc::unbounded_channel();
      // let (req_tx, req_rx) = mpsc::unbounded_channel();
      let tracer = EbpfTracer::builder()
        .mode(tracer_mode)
        .printer(printer)
        .baseline(baseline)
        .modifier(modifier_args)
        .filter(tracer_event_args.filter()?)
        .user(user)
        .tracer_tx(tracer_tx)
        .build();
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
      let mut output = Cli::get_output(output, color)?;
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
      let printer = Arc::new(Printer::new(
        PrinterArgs::from_cli(&log_args, &modifier_args),
        baseline.clone(),
      ));
      let (tx, mut rx) = mpsc::unbounded_channel();
      let tracer = EbpfTracer::builder()
        .mode(TracerMode::Log {
          foreground: log_args.foreground(),
        })
        .modifier(modifier_args)
        .printer(printer)
        .baseline(baseline.clone())
        .tracer_tx(tx)
        .user(user)
        .build();
      let running_tracer = tracer.spawn(&cmd, obj, None)?;
      let tracer_thread = spawn_blocking(move || {
        running_tracer.run_until_exit();
      });
      match format {
        ExportFormat::Json => {
          let mut json = export::Json {
            meta: JsonMetaData::new(baseline.as_ref().to_owned()),
            events: Vec::new(),
          };
          loop {
            match rx.recv().await {
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::TraceeExit { exit_code, .. },
                ..
              })) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                serialize_json_to_output(&mut output, &json, pretty)?;
                output.write_all(b"\n")?;
                output.flush()?;
                process::exit(exit_code);
              }
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::Exec(exec),
                id,
              })) => {
                json.events.push(JsonExecEvent::new(id, *exec));
              }
              // channel closed abnormally.
              None | Some(TracerMessage::FatalError(_)) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                process::exit(1);
              }
              _ => (),
            }
          }
        }
        ExportFormat::JsonStream => {
          serialize_json_to_output(
            &mut output,
            &JsonMetaData::new(baseline.as_ref().to_owned()),
            pretty,
          )?;
          loop {
            match rx.recv().await {
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::TraceeExit { exit_code, .. },
                ..
              })) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                process::exit(exit_code);
              }
              Some(TracerMessage::Event(TracerEvent {
                details: TracerEventDetails::Exec(exec),
                id,
              })) => {
                let json_event = JsonExecEvent::new(id, *exec);
                serialize_json_to_output(&mut output, &json_event, pretty)?;
                output.write_all(b"\n")?;
                output.flush()?;
              }
              // channel closed abnormally.
              None | Some(TracerMessage::FatalError(_)) => {
                tracing::debug!("Waiting for tracer thread to exit");
                tracer_thread.await?;
                process::exit(1);
              }
              _ => (),
            }
          }
        }
      }
    }
  }
}

fn utf8_lossy_cow_from_bytes_with_nul(data: &[u8]) -> Cow<str> {
  String::from_utf8_lossy(CStr::from_bytes_until_nul(data).unwrap().to_bytes())
}

fn cached_cow(cow: Cow<str>) -> ArcStr {
  match cow {
    Cow::Borrowed(s) => CACHE.write().unwrap().get_or_insert(s),
    Cow::Owned(s) => CACHE.write().unwrap().get_or_insert_owned(s),
  }
}

static CACHE: LazyLock<RwLock<StringCache>> = LazyLock::new(|| RwLock::new(StringCache::new()));
