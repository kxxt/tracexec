use std::{io::Write, sync::Arc};

use tokio::sync::mpsc::UnboundedReceiver;

use crate::{cli::args::ExporterArgs, event::TracerMessage, proc::BaselineInfo};

pub struct ExporterMetadata {
  pub baseline: Arc<BaselineInfo>,
  pub exporter_args: ExporterArgs,
}

pub trait Exporter: Sized {
  type Error;

  fn new(
    output: Box<dyn Write + Send + Sync + 'static>,
    meta: ExporterMetadata,
    stream: UnboundedReceiver<TracerMessage>,
  ) -> Result<Self, Self::Error>;

  #[allow(async_fn_in_trait)]
  async fn run(self) -> Result<i32, Self::Error>;
}
