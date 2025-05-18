use std::borrow::Cow;

use clap::ValueEnum;
use nutype::nutype;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::cli::{args::OpenTelemetryArgs, config::OpenTelemetryConfig};

mod exporter_mux;
pub mod tracer;

#[nutype(
  derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize),
  validate(with = validate_otel_exporter, error = std::borrow::Cow<'static,str> ),
)]
struct OtelExporter(Url);

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq, Deserialize, Serialize, Default)]
#[clap(rename_all = "kebab_case")]
#[serde(rename_all = "kebab-case")]
pub enum OtelSpanEndAt {
  /// If an exec event tears down the old program, end that span and start a new one
  #[default]
  Exec,
  /// Only end the span when the process exits
  Exit,
}

#[derive(Debug, Clone, ValueEnum)]
#[clap(rename_all = "kebab_case")]
pub enum OtelProtocol {
  Http,
  Grpc,
}

#[derive(Debug, Clone)]
pub enum OtelProtocolConfig {
  Http { endpoint: Option<String> },
  Grpc { endpoint: Option<String> },
}

/// Final Otel configuration with comblined options
/// from command line and config file.
#[derive(Debug, Clone, Default)]
pub struct OtelConfig {
  pub enabled_protocol: Option<OtelProtocolConfig>,
  pub service_name: Option<String>,
  pub export: OtelExport,
  pub span_end_at: OtelSpanEndAt,
}

#[derive(Debug, Clone, Default)]
pub struct OtelExport {
  pub env: bool,
  pub env_diff: bool,
  pub fd: bool,
  pub fd_diff: bool,
  pub cmdline: bool,
  pub argv: bool,
}

impl OtelConfig {
  pub fn from_cli_and_config(cli: OpenTelemetryArgs, config: OpenTelemetryConfig) -> Self {
    macro_rules! fallback {
      ($cli:ident, $config:ident, $x:ident, $default:literal) => {
        ::paste::paste! {
          match ($cli.[<otel_export_ $x>], cli.[<otel_no_export_ $x>]) {
            (false, true) => false,
            (true, false) => true,
            (false, false) => $config.export.as_ref().and_then(|e| e.$x).unwrap_or($default),
            _ => unreachable!(),
          }
        }
      };
    }
    // Merge protocol config
    let enabled_protocol = (!cli.disable_otel)
      .then(|| match cli.enable_otel {
        Some(OtelProtocol::Http) => Some(OtelProtocolConfig::Http {
          endpoint: cli
            .otel_endpoint
            .or_else(|| config.http.and_then(|v| v.endpoint)),
        }),
        Some(OtelProtocol::Grpc) => Some(OtelProtocolConfig::Grpc {
          endpoint: cli
            .otel_endpoint
            .or_else(|| config.grpc.and_then(|v| v.endpoint)),
        }),
        None => None,
      })
      .flatten();
    let service_name = cli.otel_service_name.or(config.service_name);
    // Merge export config
    let export = OtelExport {
      env: fallback!(cli, config, env, true),
      env_diff: fallback!(cli, config, env_diff, false),
      fd: fallback!(cli, config, fd, true),
      fd_diff: fallback!(cli, config, fd_diff, true),
      cmdline: fallback!(cli, config, cmdline, false),
      argv: fallback!(cli, config, argv, true),
    };
    let span_end_at = cli
      .otel_span_end_at
      .or(config.span_end_at)
      .unwrap_or_default();
    Self {
      enabled_protocol,
      service_name,
      export,
      span_end_at,
    }
  }
}

fn validate_otel_exporter(input: &Url) -> Result<(), Cow<'static, str>> {
  // match input.scheme() {
  //   // https://github.com/grpc/grpc/blob/master/doc/naming.md
  // }
  // todo!()
  Ok(())
}
