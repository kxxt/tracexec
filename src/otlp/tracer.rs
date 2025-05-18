use std::{
  borrow::Cow,
  cell::{Ref, RefCell},
  collections::BTreeMap,
  rc::Rc,
};

use itertools::Itertools;
use opentelemetry::{
  Context, InstrumentationScope, KeyValue, StringValue, Value,
  global::{BoxedSpan, BoxedTracer, ObjectSafeTracerProvider},
  trace::{Span, SpanBuilder, TraceContextExt, Tracer},
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
  error::OTelSdkResult,
  trace::{SdkTracerProvider, SpanLimits},
};

use crate::{
  cache::ArcStr,
  event::{ExecEvent, OutputMsg},
  proc::{BaselineInfo, FileDescriptorInfoCollection, Interpreter},
};

use super::{OtlpConfig, OtlpExport, OtlpProtocolConfig, OtlpSpanEndAt};

#[derive(Debug, Default)]
pub struct OtlpTracer {
  inner: RefCell<Option<OtlpTracerInner>>,
  export: OtlpExport,
  span_end_at: OtlpSpanEndAt,
}

#[derive(Debug)]
/// The [`OtlpTracerInner`] needs manual shutdown.
///
/// It is not implemented with [`Drop`] because
/// in drop we cannot properly handle the error.
pub struct OtlpTracerInner {
  provider: SdkTracerProvider,
  tracer: BoxedTracer,
  root_ctx: Rc<RefCell<Context>>,
}

impl OtlpTracerInner {
  fn shutdown(&self) -> OTelSdkResult {
    self.root_ctx.borrow().span().end();
    self.provider.shutdown()?;
    Ok(())
  }
}

impl OtlpTracer {
  pub fn new(config: OtlpConfig, baseline: &BaselineInfo) -> color_eyre::Result<Self> {
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
          inner: RefCell::new(None),
          export: config.export,
          span_end_at: config.span_end_at,
        });
      }
    };
    let provider = {
      SdkTracerProvider::builder()
        .with_span_limits(SpanLimits {
          max_events_per_span: u32::MAX,
          max_attributes_per_span: u32::MAX,
          max_links_per_span: u32::MAX,
          max_attributes_per_event: u32::MAX,
          max_attributes_per_link: u32::MAX,
        })
        .with_batch_exporter(exporter)
        .build()
    };
    let tracer = BoxedTracer::new(
      provider.boxed_tracer(
        InstrumentationScope::builder("tracexec")
          .with_version(env!("CARGO_PKG_VERSION"))
          .build(),
      ),
    );

    let serv_name = if let Some(serv_name) = config.service_name {
      Cow::Owned(serv_name)
    } else {
      Cow::Borrowed("tracexec tracer")
    };
    let mut span = tracer.build(SpanBuilder::from_name(serv_name.clone()).with_attributes([
      KeyValue::new("service.name", serv_name),
      KeyValue::new("exec.cwd", &baseline.cwd),
    ]));
    span.set_attributes(Self::env_attrs(&baseline.env));

    Ok(Self {
      inner: RefCell::new(Some(OtlpTracerInner {
        provider,
        tracer,
        root_ctx: Rc::new(RefCell::new(Context::current_with_span(span))),
      })),
      export: config.export,
      span_end_at: config.span_end_at,
    })
  }

  fn env_attrs(env: &BTreeMap<OutputMsg, OutputMsg>) -> impl IntoIterator<Item = KeyValue> {
    env
      .iter()
      .map(|(k, v)| KeyValue::new(format!("exec.env.{k}"), v))
  }

  fn append_fd_attrs(fds: &FileDescriptorInfoCollection, span: &mut BoxedSpan) {
    for (&fd, info) in fds.fdinfo.iter() {
      span.set_attributes([
        KeyValue::new(format!("exec.fd.{fd}"), &info.path),
        KeyValue::new(format!("exec.fd.{fd}.pos"), info.pos as i64),
        KeyValue::new(format!("exec.fd.{fd}.ino"), info.ino as i64),
        KeyValue::new(format!("exec.fd.{fd}.mnt"), &info.mnt),
        KeyValue::new(format!("exec.fd.{fd}.flags"), info.flags.bits() as i64),
      ]);
    }
  }

  pub fn finalize(&self) -> OTelSdkResult {
    if let Some(inner) = self.inner.borrow_mut().take() {
      inner.shutdown()?;
    }
    Ok(())
  }

  /// Create a new exec context, optionally with a parent context
  pub fn new_exec_ctx(
    &self,
    exec: &ExecEvent,
    ctx: Option<Ref<Context>>,
  ) -> Option<Rc<RefCell<Context>>> {
    let this = self.inner.borrow();
    let Some(this) = this.as_ref() else {
      return None;
    };
    let span_builder = this
      .tracer
      .span_builder(exec.filename.to_string())
      .with_start_time(exec.timestamp)
      .with_attributes([
        KeyValue::new(
          "service.name",
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
        span.set_attributes(Self::env_attrs(env));
      } else {
        span.set_attribute(KeyValue::new("warning.env", "Failed to inspect"));
      }
    }
    if self.export.argv {
      if let Ok(argv) = exec.argv.as_ref() {
        span.set_attribute(KeyValue::new(
          "exec.argv",
          Value::Array(argv.iter().map(StringValue::from).collect_vec().into()),
        ));
      } else {
        span.set_attribute(KeyValue::new("warning.argv", "Failed to inspect"));
      }
    }
    if self.export.cmdline {
      todo!()
    }
    if self.export.fd {
      Self::append_fd_attrs(&exec.fdinfo, &mut span);
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

    // span.set_attribute(KeyValue::new(
    //   "_service.name",
    //   exec
    //     .argv
    //     .as_deref()
    //     .ok()
    //     .and_then(|v| v.first())
    //     .unwrap_or(&exec.filename),
    // ));
    Some(Rc::new(RefCell::new(Context::current_with_span(span))))
  }

  pub fn span_could_end_at_exec(&self) -> bool {
    self.span_end_at == OtlpSpanEndAt::Exec
  }

  pub fn root_ctx(&self) -> Option<Rc<RefCell<Context>>> {
    self.inner.borrow().as_ref().map(|e| e.root_ctx.clone())
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
      Self::None => "none".into(),
      Self::Shebang(arc_str) => arc_str.clone_inner().into(),
      Self::ExecutableUnaccessible => "err: inaccessible".into(),
      Self::Error(arc_str) => format!("err: {arc_str}").into(),
    }
  }
}
