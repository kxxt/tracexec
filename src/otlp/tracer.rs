use std::{
  borrow::Cow,
  cell::{Ref, RefCell},
  collections::BTreeMap,
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
use opentelemetry_sdk::{
  Resource,
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
  root_ctx: Option<Rc<RefCell<Context>>>,
}

#[derive(Debug)]
pub struct OtlpTracerInner {
  provider: SdkTracerProvider,
  tracer: BoxedTracer,
}

impl Drop for OtlpTracer {
  fn drop(&mut self) {
    if let Some(ctx) = &self.root_ctx {
      ctx.borrow_mut().span().end();
    }
  }
}

impl Drop for OtlpTracerInner {
  fn drop(&mut self) {
    self
      .provider
      .shutdown()
      .expect("Failed to shutdown OpenTelemetry provider")
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
          root_ctx: None,
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
      inner: RefCell::new(Some(OtlpTracerInner { provider, tracer })),
      export: config.export,
      span_end_at: config.span_end_at,
      root_ctx: Some(Rc::new(RefCell::new(Context::current_with_span(span)))),
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

  pub fn finalize(&self) {
    drop(self.inner.borrow_mut().take());
  }

  /// Create a new exec context, optionally with a parent context
  pub fn new_exec_ctx(
    &self,
    exec: &ExecEvent,
    ctx: Option<Ref<Context>>,
  ) -> Option<Rc<RefCell<Context>>> {
    if ctx.is_none() {
      panic!("Should have a parent");
    }
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
      } else {
      }
    }
    if self.export.fd_diff {}
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
    self.root_ctx.clone()
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
