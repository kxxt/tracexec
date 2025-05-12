use std::{borrow::Cow, time::SystemTime};

use chrono::Utc;
use clap::ValueEnum;
use nutype::nutype;
use opentelemetry::{
  Context, KeyValue, global,
  trace::{Span, SpanBuilder, TraceContextExt, Tracer},
};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::Resource;
use url::Url;

use crate::{
  cli::{
    args::OpenTelemetryArgs,
    config::{OpenTelemetryConfig, OtelExportConfig},
  },
  export,
};

mod exporter_mux;
pub mod tracer;

#[nutype(
  derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize),
  validate(with = validate_otlp_exporter, error = std::borrow::Cow<'static,str> ),
)]
struct OtlpExporter(Url);

#[derive(Debug, Clone, ValueEnum)]
#[clap(rename_all = "kebab_case")]
pub enum OtlpProtocol {
  Http,
  Grpc,
}

#[derive(Debug, Clone)]
pub enum OtlpProtocolConfig {
  Http { endpoint: Option<String> },
  Grpc { endpoint: Option<String> },
}

/// Final Otlp configuration with comblined options
/// from command line and config file.
#[derive(Debug, Clone)]
pub struct OtlpConfig {
  pub enabled_protocol: Option<OtlpProtocolConfig>,
  pub service_name: Option<String>,
  pub export: OtlpExport,
}

#[derive(Debug, Clone, Default)]
pub struct OtlpExport {
  pub env: bool,
  pub env_diff: bool,
  pub fd: bool,
  pub fd_diff: bool,
  pub cmdline: bool,
  pub argv: bool,
}

impl OtlpConfig {
  pub fn from_cli_and_config(cli: OpenTelemetryArgs, config: OpenTelemetryConfig) -> Self {
    macro_rules! fallback {
      ($cli:ident, $config:ident, $x:ident, $default:literal) => {
        ::paste::paste! {
          match ($cli.[<otlp_export_ $x>], cli.[<otlp_no_export_ $x>]) {
            (false, true) => false,
            (true, false) => true,
            (false, false) => $config.export.as_ref().and_then(|e| e.$x).unwrap_or($default),
            _ => unreachable!(),
          }
        }
      };
    }
    // Merge protocol config
    let enabled_protocol = (!cli.disable_otlp)
      .then(|| match cli.enable_otlp {
        Some(OtlpProtocol::Http) => Some(OtlpProtocolConfig::Http {
          endpoint: cli.otlp_endpoint.or(config.http.and_then(|v| v.endpoint)),
        }),
        Some(OtlpProtocol::Grpc) => Some(OtlpProtocolConfig::Grpc {
          endpoint: cli.otlp_endpoint.or(config.grpc.and_then(|v| v.endpoint)),
        }),
        None => None,
      })
      .flatten();
    let service_name = cli.otlp_service_name.or(config.service_name);
    // Merge export config
    let export = OtlpExport {
      env: fallback!(cli, config, env, true),
      env_diff: fallback!(cli, config, env_diff, false),
      fd: fallback!(cli, config, fd, true),
      fd_diff: fallback!(cli, config, fd_diff, true),
      cmdline: fallback!(cli, config, cmdline, false),
      argv: fallback!(cli, config, argv, true),
    };
    Self {
      enabled_protocol,
      service_name,
      export,
    }
  }
}

fn validate_otlp_exporter(input: &Url) -> Result<(), Cow<'static, str>> {
  // match input.scheme() {
  //   // https://github.com/grpc/grpc/blob/master/doc/naming.md
  // }
  todo!()
}
