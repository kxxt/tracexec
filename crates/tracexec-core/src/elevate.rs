//! Privilege elevation support for tracexec.
//!
//! When `--elevate` is used, tracexec captures the current user's credentials,
//! creates a private abstract Unix domain socket, and spawns `sudo tracexec`.
//! The elevated child requests the complete original environment over that
//! socket. The unelevated parent verifies the child's Unix socket credentials
//! before sending anything, then waits for the elevated child to exit. The
//! elevated tracexec process may only consult an allowlisted subset for its own
//! behavior, while the tracee is spawned with the complete original environment.

use std::{
  collections::HashSet,
  ffi::{
    OsStr,
    OsString,
  },
  io::{
    ErrorKind,
    Read,
    Write,
  },
  os::{
    linux::net::SocketAddrExt,
    unix::{
      ffi::{
        OsStrExt,
        OsStringExt,
      },
      net::{
        SocketAddr,
        UnixListener,
        UnixStream,
      },
      process::ExitStatusExt,
    },
  },
  path::Path,
  process::{
    Child,
    Command,
    ExitStatus,
  },
  sync::LazyLock,
  time::{
    Duration,
    Instant,
  },
};

use color_eyre::eyre::bail;
use nix::{
  sys::socket::{
    getsockopt,
    sockopt,
  },
  unistd::{
    Uid,
    User,
  },
};
use rand::distr::{
  Alphanumeric,
  SampleString,
};

pub type EnvVars = Vec<(OsString, OsString)>;

const ENV_REQUEST_MAGIC: &[u8] = b"tracexec-env-v1";
const ENV_SOCKET_ACCEPT_TIMEOUT: Duration = Duration::from_secs(180);

/// Environment variables elevated tracexec may consult after `--elevate`.
///
/// Keep this list limited to variables that tracexec itself reads. The complete
/// original environment is transferred as data, but only these keys are exposed
/// to elevated tracexec behavior. The tracee still receives the complete
/// original environment at `execve(2)`.
///
/// `TRACEXEC_DATA` is passed via cmdline and thus not allowed here.
/// Variables only for development, like `TRACEXEC_BPFCOV_OUTDIR`,
/// are also not allowed.
pub static RESTORED_ENV_ALLOWLIST: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
  HashSet::from([
    "NO_COLOR",
    "RUST_LOG",
    "TRACEXEC_LOG_LEVEL",
    "TRACEXEC_NO_SLEEP",
    "TRACEXEC_USE_FENTRY",
    "TRACEXEC_USE_KPROBE",
  ])
});

/// Saved credentials from before privilege elevation.
#[derive(Debug, Clone)]
pub struct PreElevationCreds {
  pub username: String,
  pub uid: u32,
  pub gid: u32,
}

impl PreElevationCreds {
  /// Capture the current process's real credentials.
  pub fn capture() -> color_eyre::Result<Self> {
    let uid = nix::unistd::getuid();
    let gid = nix::unistd::getgid();
    let user = User::from_uid(uid)?
      .ok_or_else(|| color_eyre::eyre::eyre!("Failed to look up current user (uid={uid})"))?;
    Ok(Self {
      username: user.name,
      uid: uid.as_raw(),
      gid: gid.as_raw(),
    })
  }
}

pub fn env_value<'a>(env: &'a [(OsString, OsString)], key: &str) -> Option<&'a OsStr> {
  let key = OsStr::new(key);
  env
    .iter()
    .rev()
    .find_map(|(candidate, value)| (candidate == key).then_some(value.as_os_str()))
}

pub fn env_var_os(env: Option<&[(OsString, OsString)]>, key: &str) -> Option<OsString> {
  match env {
    Some(env) => env_value(env, key).map(OsStr::to_owned),
    None => std::env::var_os(key),
  }
}

pub fn env_var_string(env: Option<&[(OsString, OsString)]>, key: &str) -> Option<String> {
  match env {
    Some(env) => env_value(env, key).map(|value| value.to_string_lossy().into_owned()),
    None => std::env::var(key).ok(),
  }
}

pub fn filter_allowlisted_env_from(
  vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> EnvVars {
  vars
    .into_iter()
    .filter(|(key, _)| {
      key
        .to_str()
        .is_some_and(|key| RESTORED_ENV_ALLOWLIST.contains(key))
    })
    .collect()
}

pub fn filter_allowlisted_env(env: &[(OsString, OsString)]) -> EnvVars {
  filter_allowlisted_env_from(env.iter().cloned())
}

fn collect_original_env() -> EnvVars {
  std::env::vars_os().collect()
}

/// Serialize environment variables into a byte buffer.
///
/// Uses the null-byte-separated `KEY=VALUE\0` format.
fn serialize_env(env: &[(OsString, OsString)]) -> Vec<u8> {
  let mut buf = Vec::new();
  for (key, value) in env {
    buf.extend_from_slice(key.as_bytes());
    buf.push(b'=');
    buf.extend_from_slice(value.as_bytes());
    buf.push(0);
  }
  buf
}

/// Deserialize environment variables from null-byte-separated `KEY=VALUE\0` format.
fn deserialize_env(data: &[u8]) -> EnvVars {
  let mut result = Vec::new();
  for entry in data.split(|&b| b == 0) {
    if entry.is_empty() {
      continue;
    }
    if let Some(eq_pos) = entry.iter().position(|&b| b == b'=') {
      let key = OsString::from_vec(entry[..eq_pos].to_vec());
      let value = OsString::from_vec(entry[eq_pos + 1..].to_vec());
      result.push((key, value));
    }
  }
  result
}

fn abstract_socket_addr(socket_name: &str) -> color_eyre::Result<SocketAddr> {
  Ok(SocketAddr::from_abstract_name(socket_name.as_bytes())?)
}

fn random_socket_name() -> String {
  let suffix = Alphanumeric.sample_string(&mut rand::rng(), 32);
  format!("tracexec-env-{}-{suffix}", std::process::id())
}

fn bind_env_socket(socket_name: &str) -> color_eyre::Result<UnixListener> {
  Ok(UnixListener::bind_addr(&abstract_socket_addr(
    socket_name,
  )?)?)
}

fn exit_code_from_status(status: ExitStatus) -> i32 {
  status
    .code()
    .or_else(|| status.signal().map(|signal| 128 + signal))
    .unwrap_or(1)
}

fn handle_env_request(
  mut stream: UnixStream,
  env: &[(OsString, OsString)],
  required_uid: u32,
) -> color_eyre::Result<()> {
  let creds = getsockopt(&stream, sockopt::PeerCredentials)?;
  if creds.uid() != required_uid {
    bail!(
      "Refusing to send environment variables to uid {} (expected uid {required_uid})",
      creds.uid()
    );
  }

  let mut request = vec![0; ENV_REQUEST_MAGIC.len()];
  stream.read_exact(&mut request)?;
  if request != ENV_REQUEST_MAGIC {
    bail!("Invalid request");
  }

  let payload = serialize_env(env);
  stream.write_all(&(payload.len() as u32).to_be_bytes())?;
  stream.write_all(&payload)?;
  stream.flush()?;
  Ok(())
}

fn serve_env_to_child(
  listener: &UnixListener,
  child: &mut Child,
  env: &[(OsString, OsString)],
  timeout: Duration,
) -> color_eyre::Result<Option<ExitStatus>> {
  listener.set_nonblocking(true)?;
  let deadline = Instant::now() + timeout;

  loop {
    if let Some(status) = child.try_wait()? {
      return Ok(Some(status));
    }

    match listener.accept() {
      Ok((stream, _)) => {
        handle_env_request(stream, env, 0)?;
        return Ok(None);
      }
      Err(e) if e.kind() == ErrorKind::WouldBlock => {
        if Instant::now() >= deadline {
          bail!("Timed out waiting for the elevated subprocess to request environment variables");
        }
        std::thread::sleep(Duration::from_millis(50));
      }
      Err(e) => return Err(e.into()),
    }
  }
}

/// Request the complete original environment from the unelevated parent.
pub fn request_env_from_parent(socket_name: &str) -> color_eyre::Result<EnvVars> {
  let mut stream = UnixStream::connect_addr(&abstract_socket_addr(socket_name)?)?;
  stream.write_all(ENV_REQUEST_MAGIC)?;
  stream.flush()?;

  let mut len = [0; 4];
  stream.read_exact(&mut len)?;
  let len = u32::from_be_bytes(len) as usize;
  let mut payload = vec![0; len];
  stream.read_exact(&mut payload)?;
  Ok(deserialize_env(&payload))
}

/// Construct the command line for re-execution with elevation.
///
/// Transforms `tracexec --elevate [opts] <subcommand> [args] -- <cmd...>`
/// into
///
/// ```bash
/// sudo tracexec --user <username> --restore-env-socket <socket-name> \
///   --elevated-config-dir <path> --elevated-data-dir <path> \
///   --elevated-data-local-dir <path> [opts] <subcommand> [args] \
///   -- <cmd...>
/// ```
fn build_elevated_args(
  creds: &PreElevationCreds,
  socket_name: &str,
  config_dir: Option<&Path>,
  data_dir: Option<&Path>,
  data_local_dir: Option<&Path>,
) -> Vec<OsString> {
  build_elevated_args_from(
    std::env::args_os(),
    creds,
    socket_name,
    config_dir,
    data_dir,
    data_local_dir,
  )
}

/// Testable inner implementation of [`build_elevated_args`].
///
/// `args` should include argv\[0\] (the program name), which is skipped.
fn build_elevated_args_from(
  args: impl IntoIterator<Item = impl Into<OsString>>,
  creds: &PreElevationCreds,
  socket_name: &str,
  config_dir: Option<&Path>,
  data_dir: Option<&Path>,
  data_local_dir: Option<&Path>,
) -> Vec<OsString> {
  let mut result = Vec::new();
  let mut replaced = false;
  let mut past_delimiter = false;

  for arg in args.into_iter().skip(1) {
    let arg: OsString = arg.into();
    if past_delimiter {
      // After "--", pass everything through verbatim.
      result.push(arg);
      continue;
    }
    if arg == "--" {
      past_delimiter = true;
      result.push(arg);
      continue;
    }
    if !replaced && arg == "--elevate" {
      // Replace the first --elevate with --user/--restore-env-socket and optional dir overrides.
      replaced = true;
      result.push(OsString::from("--user"));
      result.push(OsString::from(&creds.username));
      result.push(OsString::from("--restore-env-socket"));
      result.push(OsString::from(socket_name));
      if let Some(dir) = config_dir {
        result.push(OsString::from("--elevated-config-dir"));
        result.push(dir.as_os_str().to_owned());
      }
      if let Some(dir) = data_dir {
        result.push(OsString::from("--elevated-data-dir"));
        result.push(dir.as_os_str().to_owned());
      }
      if let Some(dir) = data_local_dir {
        result.push(OsString::from("--elevated-data-local-dir"));
        result.push(dir.as_os_str().to_owned());
      }
    } else {
      result.push(arg);
    }
  }

  result
}

/// Re-execute tracexec with elevated privileges via sudo.
///
/// This function does not return on success: the unelevated parent exits with
/// the same status as the elevated child.
pub fn elevate_and_reexec() -> color_eyre::Result<std::convert::Infallible> {
  if Uid::effective().is_root() {
    color_eyre::eyre::bail!("--elevate is not needed when already running as root");
  }

  let creds = PreElevationCreds::capture()?;
  let socket_name = random_socket_name();
  let listener = bind_env_socket(&socket_name)?;
  let env = collect_original_env();
  let exe = std::env::current_exe()?;

  // Capture the current user's project directories so the elevated process
  // can use them instead of root's directories.
  let proj_dirs = crate::cli::config::project_directory();
  let config_dir = proj_dirs.as_ref().map(|d| d.config_dir().to_path_buf());
  let data_dir = proj_dirs.as_ref().map(|d| d.data_dir().to_path_buf());
  let data_local_dir = proj_dirs.as_ref().map(|d| d.data_local_dir().to_path_buf());
  let elevated_args = build_elevated_args(
    &creds,
    &socket_name,
    config_dir.as_deref(),
    data_dir.as_deref(),
    data_local_dir.as_deref(),
  );

  tracing::debug!(
    "Elevating: sudo {} {}",
    exe.display(),
    elevated_args
      .iter()
      .map(|a| a.to_string_lossy().to_string())
      .collect::<Vec<_>>()
      .join(" ")
  );

  let mut child = Command::new("sudo")
    .arg(&exe)
    .args(&elevated_args)
    .spawn()?;
  match serve_env_to_child(&listener, &mut child, &env, ENV_SOCKET_ACCEPT_TIMEOUT) {
    Ok(Some(status)) => std::process::exit(exit_code_from_status(status)),
    Ok(None) => {
      drop(listener);
      let status = child.wait()?;
      std::process::exit(exit_code_from_status(status));
    }
    Err(e) => {
      let _ = child.kill();
      let _ = child.wait();
      Err(e)
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    os::unix::ffi::OsStringExt,
    thread,
  };

  use super::*;

  fn test_creds() -> PreElevationCreds {
    PreElevationCreds {
      username: "testuser".to_string(),
      uid: 1000,
      gid: 1000,
    }
  }

  #[test]
  fn test_capture_creds() {
    let creds = PreElevationCreds::capture().unwrap();
    assert_eq!(creds.uid, nix::unistd::getuid().as_raw());
    assert_eq!(creds.gid, nix::unistd::getgid().as_raw());
    assert!(!creds.username.is_empty());
  }

  #[test]
  fn test_allowlist_filters_unneeded_env_vars() {
    let filtered = filter_allowlisted_env_from([
      (OsString::from("TRACEXEC_NO_SLEEP"), OsString::from("1")),
      (
        OsString::from("TRACEXEC_TEST_MARKER"),
        OsString::from("secret"),
      ),
      (OsString::from("PATH"), OsString::from("/usr/bin")),
    ]);

    assert_eq!(
      filtered,
      vec![(OsString::from("TRACEXEC_NO_SLEEP"), OsString::from("1"),),]
    );
  }

  #[test]
  fn test_serialize_deserialize_roundtrip() {
    let original = vec![
      (OsString::from("TRACEXEC_NO_SLEEP"), OsString::from("1")),
      (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
      (OsString::from("EMPTY"), OsString::from("")),
      (OsString::from("MULTI_EQ"), OsString::from("a=b=c")),
    ];

    let data = serialize_env(&original);
    let deserialized = deserialize_env(&data);
    assert_eq!(deserialized, original);
  }

  #[test]
  fn test_deserialize_env_ignores_malformed() {
    // Entries without '=' are silently skipped
    let data = b"GOOD=value\0BAD_NO_EQUAL\0ALSO_GOOD=\0";
    let result = deserialize_env(data);
    assert_eq!(
      result,
      vec![
        (OsString::from("GOOD"), OsString::from("value")),
        (OsString::from("ALSO_GOOD"), OsString::from("")),
      ]
    );
  }

  #[test]
  fn test_deserialize_env_handles_non_utf8() {
    // Environment variables can contain non-UTF-8 bytes on Unix
    let mut data = Vec::new();
    data.extend_from_slice(b"KEY=");
    data.extend_from_slice(&[0xff, 0xfe]); // non-UTF-8
    data.push(0);

    let result = deserialize_env(&data);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, OsString::from("KEY"));
    assert_eq!(result[0].1, OsString::from_vec(vec![0xff, 0xfe]));
  }

  #[test]
  fn test_env_socket_roundtrip() {
    let socket_name = random_socket_name();
    let listener = bind_env_socket(&socket_name).unwrap();
    let env = vec![
      (OsString::from("TRACEXEC_NO_SLEEP"), OsString::from("1")),
      (OsString::from("PATH"), OsString::from("/usr/bin")),
    ];
    let expected = env.clone();
    let uid = nix::unistd::getuid().as_raw();

    let server = thread::spawn(move || {
      let (stream, _) = listener.accept().unwrap();
      handle_env_request(stream, &env, uid).unwrap();
    });

    let received = request_env_from_parent(&socket_name).unwrap();
    server.join().unwrap();
    assert_eq!(received, expected);
  }

  #[test]
  fn test_build_elevated_args_replaces_elevate() {
    let creds = test_creds();
    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("tui"),
      OsString::from("-t"),
      OsString::from("--"),
      OsString::from("sudo"),
      OsString::from("ls"),
    ];

    let result = build_elevated_args_from(input_args, &creds, "sock-name", None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env-socket"),
        OsString::from("sock-name"),
        OsString::from("tui"),
        OsString::from("-t"),
        OsString::from("--"),
        OsString::from("sudo"),
        OsString::from("ls"),
      ]
    );
  }

  #[test]
  fn test_build_elevated_args_preserves_other_flags() {
    let creds = PreElevationCreds {
      username: "bob".to_string(),
      uid: 1002,
      gid: 1002,
    };
    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--color=always"),
      OsString::from("--elevate"),
      OsString::from("-C"),
      OsString::from("/tmp"),
      OsString::from("log"),
      OsString::from("--"),
      OsString::from("ls"),
    ];

    let result = build_elevated_args_from(input_args, &creds, "sock-999", None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--color=always"),
        OsString::from("--user"),
        OsString::from("bob"),
        OsString::from("--restore-env-socket"),
        OsString::from("sock-999"),
        OsString::from("-C"),
        OsString::from("/tmp"),
        OsString::from("log"),
        OsString::from("--"),
        OsString::from("ls"),
      ]
    );
  }

  #[test]
  fn test_build_elevated_args_passes_project_dirs() {
    let creds = test_creds();
    let config_dir = Path::new("/home/testuser/.config/tracexec");
    let data_dir = Path::new("/home/testuser/.local/share/tracexec");
    let data_local_dir = Path::new("/home/testuser/.local/share/tracexec");

    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("log"),
      OsString::from("--"),
      OsString::from("ls"),
    ];

    let result = build_elevated_args_from(
      input_args,
      &creds,
      "sock-abc",
      Some(config_dir),
      Some(data_dir),
      Some(data_local_dir),
    );

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env-socket"),
        OsString::from("sock-abc"),
        OsString::from("--elevated-config-dir"),
        OsString::from("/home/testuser/.config/tracexec"),
        OsString::from("--elevated-data-dir"),
        OsString::from("/home/testuser/.local/share/tracexec"),
        OsString::from("--elevated-data-local-dir"),
        OsString::from("/home/testuser/.local/share/tracexec"),
        OsString::from("log"),
        OsString::from("--"),
        OsString::from("ls"),
      ]
    );
  }

  #[test]
  fn test_build_elevated_args_ignores_elevate_after_delimiter() {
    let creds = test_creds();
    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("log"),
      OsString::from("--"),
      OsString::from("cmd"),
      OsString::from("--elevate"),
    ];

    let result = build_elevated_args_from(input_args, &creds, "sock-abc", None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env-socket"),
        OsString::from("sock-abc"),
        OsString::from("log"),
        OsString::from("--"),
        OsString::from("cmd"),
        OsString::from("--elevate"),
      ]
    );
  }
}
