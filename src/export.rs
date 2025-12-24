mod json;
mod perfetto;

use std::{io::Write, sync::Arc};

pub use json::*;
pub use perfetto::PerfettoExporter;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{event::TracerMessage, proc::BaselineInfo};

pub struct ExporterMetadata {
  pub(super) baseline: Arc<BaselineInfo>,
  pub(super) pretty: bool,
}

pub trait Exporter: Sized {
  type Error;

  fn new(
    output: Box<dyn Write + Send + Sync + 'static>,
    meta: ExporterMetadata,
    stream: UnboundedReceiver<TracerMessage>,
  ) -> Result<Self, Self::Error>;

  async fn run(self) -> Result<i32, Self::Error>;
}
