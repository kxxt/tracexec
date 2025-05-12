use std::{
  cell::{Ref, RefCell},
  rc::Rc,
};

use itertools::{Itertools, chain};
use nix::libc::ENOENT;
use opentelemetry::{
  Context, InstrumentationScope, KeyValue, StringValue, Value,
  global::{BoxedSpan, BoxedTracer, ObjectSafeTracer, ObjectSafeTracerProvider},
  trace::{Span, SpanBuilder, Status, TraceContextExt, Tracer},
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};

use crate::{
  cache::ArcStr,
  event::{ExecEvent, OutputMsg},
  proc::Interpreter,
};

use super::{OtlpConfig, OtlpExport, OtlpProtocolConfig};

#[derive(Debug, Default)]
pub struct OtlpTracer {
  inner: Option<OtlpTracerInner>,
  export: OtlpExport,
}

#[derive(Debug)]
pub struct OtlpTracerInner {
  provider: SdkTracerProvider,
  tracer: BoxedTracer,
}

/// This is an unfortunate ugly hack because OpenTelemetry standard does not
/// allow us to override the service name per span.
pub struct OtlpProviderCache {}

impl Drop for OtlpTracerInner {
  fn drop(&mut self) {
    self
      .provider
      .shutdown()
      .expect("Failed to shutdown OpenTelemetry provider")
  }
}

impl OtlpTracer {
  pub fn new(config: OtlpConfig) -> color_eyre::Result<Self> {
    let exporter = opentelemetry_otlp::SpanExporter::builder();
    let exporter = match config.enabled_protocol {
      Some(OtlpProtocolConfig::Grpc { endpoint }) => {
        let exporter = exporter.with_tonic();
        if let Some(endpoint) = endpoint {
          exporter.with_endpoint(endpoint)
        } else {
          exporter
        }
        .build()?
      }
      Some(OtlpProtocolConfig::Http { endpoint }) => {
        let exporter = exporter
          .with_http()
          .with_protocol(opentelemetry_otlp::Protocol::HttpBinary);
        if let Some(endpoint) = endpoint {
          exporter.with_endpoint(endpoint)
        } else {
          exporter
        }
        .build()?
      }
      None => {
        return Ok(Self {
          inner: None,
          export: config.export,
        });
      }
    };
    let provider = {
      let mut p = SdkTracerProvider::builder();
      if let Some(serv_name) = config.service_name {
        p = p.with_resource(Resource::builder().with_service_name(serv_name).build());
      }
      p.with_batch_exporter(exporter).build()
    };
    let tracer = BoxedTracer::new(
      provider.boxed_tracer(
        InstrumentationScope::builder("tracexec")
          .with_version(env!("CARGO_PKG_VERSION"))
          .build(),
      ),
    );
    Ok(Self {
      inner: Some(OtlpTracerInner { provider, tracer }),
      export: config.export,
    })
  }

  /// Create a new exec context, optionally with a parent context
  pub fn new_exec_ctx(
    &self,
    exec: &ExecEvent,
    ctx: Option<Ref<Context>>,
  ) -> Option<Rc<RefCell<Context>>> {
    let Some(this) = &self.inner else {
      return None;
    };
    let span_builder = this
      .tracer
      .span_builder(exec.filename.to_string())
      .with_start_time(exec.timestamp)
      .with_attributes([
        KeyValue::new(
          "resource.name",
          exec
            .argv
            .as_deref()
            .ok()
            .and_then(|v| v.first())
            .unwrap_or(&exec.filename),
        ),
        KeyValue::new("exec.filename", &exec.filename),
        KeyValue::new("exec.parent_comm", &exec.comm),
        KeyValue::new("exec.result", exec.result),
        KeyValue::new("exec.cwd", &exec.cwd),
        KeyValue::new("proc.pid", exec.pid.as_raw() as i64),
      ]);

    let mut span = if let Some(ctx) = ctx {
      this.tracer.build_with_context(span_builder, &ctx)
    } else {
      this.tracer.build(span_builder)
    };
    if let Some(interp) = exec.interpreter.as_ref() {
      span.set_attribute(KeyValue::new(
        "exec.interpreter",
        Value::Array(interp.iter().map(|i| i.as_trace()).collect_vec().into()),
      ));
    }
    if self.export.env {
      if let Ok(env) = exec.envp.as_ref() {
        span.set_attributes(
          env
            .iter()
            .map(|(k, v)| KeyValue::new(format!("exec.env.{k}"), v)),
        );
      } else {
        span.set_attribute(KeyValue::new("warning.env", "Failed to inspect"));
      }
    }
    if self.export.argv {
      if let Ok(argv) = exec.argv.as_ref() {
        span.set_attribute(KeyValue::new(
          "exec.argv",
          Value::Array(
            argv
              .iter()
              .map(|v| StringValue::from(v))
              .collect_vec()
              .into(),
          ),
        ));
      } else {
        span.set_attribute(KeyValue::new("warning.argv", "Failed to inspect"));
      }
    }
    if self.export.cmdline {
      todo!()
    }
    if self.export.fd {
      for (&fd, info) in exec.fdinfo.fdinfo.iter() {
        span.set_attributes([
          KeyValue::new(format!("exec.fd.{fd}"), &info.path),
          KeyValue::new(format!("exec.fd.{fd}.pos"), info.pos as i64),
          KeyValue::new(format!("exec.fd.{fd}.ino"), info.ino as i64),
          KeyValue::new(format!("exec.fd.{fd}.mnt"), &info.mnt),
          KeyValue::new(format!("exec.fd.{fd}.flags"), info.flags.bits() as i64),
        ]);
      }
    }
    if self.export.env_diff {
      if let Ok(diff) = exec.env_diff.as_ref() {
        for (kind, collection) in [("added", &diff.added), ("modified", &diff.modified)] {
          span.set_attributes(
            collection
              .iter()
              .map(|(k, v)| KeyValue::new(format!("exec.env_diff.{kind}.{k}"), v)),
          );
        }
        span.set_attributes(
          diff
            .removed
            .iter()
            .map(|k| KeyValue::new(format!("exec.env_diff.removed.{k}"), true)),
        );
      }
    }
    if self.export.fd_diff {}
    Some(Rc::new(RefCell::new(Context::current_with_span(span))))
  }
}

impl From<&OutputMsg> for opentelemetry::Key {
  fn from(value: &OutputMsg) -> Self {
    match value {
      OutputMsg::PartialOk(arc_str) | OutputMsg::Ok(arc_str) => Self::new(arc_str.clone_inner()),
      OutputMsg::Err(e) => Self::from_static_str(<&'static str>::from(e)),
    }
  }
}

impl From<&OutputMsg> for opentelemetry::Value {
  fn from(value: &OutputMsg) -> Self {
    StringValue::from(value).into()
  }
}

impl From<&OutputMsg> for opentelemetry::StringValue {
  fn from(value: &OutputMsg) -> Self {
    match value {
      OutputMsg::PartialOk(arc_str) | OutputMsg::Ok(arc_str) => arc_str.clone_inner().into(),
      OutputMsg::Err(e) => <&'static str>::from(e).into(),
    }
  }
}

impl From<&ArcStr> for opentelemetry::Value {
  fn from(value: &ArcStr) -> Self {
    value.clone_inner().into()
  }
}

impl Interpreter {
  pub fn as_trace(&self) -> StringValue {
    match self {
      Interpreter::None => "none".into(),
      Interpreter::Shebang(arc_str) => arc_str.clone_inner().into(),
      Interpreter::ExecutableUnaccessible => "err: inaccessible".into(),
      Interpreter::Error(arc_str) => format!("err: {arc_str}").into(),
    }
  }
}
