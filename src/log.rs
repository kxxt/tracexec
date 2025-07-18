// Copyright (c) 2023 Ratatui Developers
// Copyright (c) 2024 Levi Zim

// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
// associated documentation files (the "Software"), to deal in the Software without restriction,
// including without limitation the rights to use, copy, modify, merge, publish, distribute,
// sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all copies or substantial
// portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
// NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
// OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::{path::PathBuf, sync::LazyLock};

use color_eyre::eyre::Result;
use tracing_error::ErrorLayer;
use tracing_subscriber::{self, Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub use tracing::*;

use crate::{cli::config::project_directory, tui::restore_tui};

const LOG_FILE: &str = concat!(env!("CARGO_PKG_NAME"), ".log");

static LOG_ENV: LazyLock<String> =
  LazyLock::new(|| concat!(env!("CARGO_CRATE_NAME"), "_LOGLEVEL").to_uppercase());

pub fn get_data_dir() -> PathBuf {
  if let Some(s) = std::env::var(concat!(env!("CARGO_CRATE_NAME"), "_DATA").to_uppercase())
    .ok()
    .map(PathBuf::from)
  {
    s
  } else if let Some(proj_dirs) = project_directory() {
    proj_dirs.data_local_dir().to_path_buf()
  } else {
    PathBuf::from(".").join(".data")
  }
}

pub fn initialize_logging() -> Result<()> {
  let directory = get_data_dir();
  std::fs::create_dir_all(directory.clone())?;
  let log_path = directory.join(LOG_FILE);
  let log_file = std::fs::File::create(log_path)?;
  let file_subscriber = tracing_subscriber::fmt::layer()
    .with_file(true)
    .with_thread_ids(true)
    .with_thread_names(true)
    .with_line_number(true)
    .with_writer(log_file)
    .with_target(false)
    .with_ansi(false);

  let file_subscriber = if std::env::var(LOG_ENV.clone()).is_ok() {
    file_subscriber.with_filter(tracing_subscriber::filter::EnvFilter::from_env(
      LOG_ENV.clone(),
    ))
  } else if std::env::var("RUST_LOG").is_ok() {
    file_subscriber.with_filter(tracing_subscriber::filter::EnvFilter::from_env("RUST_LOG"))
  } else {
    file_subscriber.with_filter(tracing_subscriber::filter::EnvFilter::new(concat!(
      env!("CARGO_CRATE_NAME"),
      "=info"
    )))
  };

  tracing_subscriber::registry()
    .with(file_subscriber)
    .with(ErrorLayer::default())
    .init();
  Ok(())
}

/// Similar to the `std::dbg!` macro, but generates `tracing` events rather
/// than printing to stdout.
///
/// By default, the verbosity level for the generated events is `DEBUG`, but
/// this can be customized.
#[macro_export]
macro_rules! trace_dbg {
    (target: $target:expr, level: $level:expr, $ex:expr) => {{
        match $ex {
            value => {
                tracing::event!(target: $target, $level, ?value, stringify!($ex));
                value
            }
        }
    }};
    (level: $level:expr, $ex:expr) => {
        trace_dbg!(target: module_path!(), level: $level, $ex)
    };
    (target: $target:expr, $ex:expr) => {
        trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
    };
    ($ex:expr) => {
        trace_dbg!(level: tracing::Level::DEBUG, $ex)
    };
}

pub fn initialize_panic_handler() {
  std::panic::set_hook(Box::new(|panic_info| {
    if let Err(e) = restore_tui() {
      error!("Unable to restore Terminal: {e:?}");
    }
    better_panic::Settings::auto()
      .most_recent_first(false)
      .lineno_suffix(true)
      .verbosity(better_panic::Verbosity::Full)
      .create_panic_handler()(panic_info);
    std::process::exit(nix::libc::EXIT_FAILURE);
  }));
}
