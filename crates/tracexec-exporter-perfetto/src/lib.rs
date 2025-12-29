mod intern;
mod packet;
mod perfetto;
mod producer;
mod recorder;

#[cfg(not(feature = "protobuf-binding-from-source"))]
mod proto;
#[cfg(feature = "protobuf-binding-from-source")]
mod proto {
  pub use perfetto_trace_proto::*;
}

pub use perfetto::PerfettoExporter;
