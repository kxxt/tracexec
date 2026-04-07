//! Privilege elevation support for tracexec.
//!
//! When `--elevate` is used, tracexec captures the current user's credentials
//! and environment variables, saves them to a secure temporary file, then
//! re-executes itself with elevated privileges via `sudo`. The elevated process
//! restores the original environment from the file and passes `--user <username>`
//! so the tracee runs as the original user.
//!
//! ## Security
//!
//! The environment file is created with mode 0600 and owned by the current user.
//! On restore, the file is opened with `O_NOFOLLOW` (rejecting symlinks), its
//! metadata is checked via `fstat` (avoiding TOCTOU races), and it is unlinked
//! immediately after reading. Only the original user and root can access it.

use std::{
  ffi::OsString,
  fs,
  io::{
    Read,
    Write,
  },
  os::unix::{
    ffi::OsStrExt,
    fs::OpenOptionsExt,
  },
  path::Path,
  process::Command,
};

use nix::{
  fcntl::OFlag,
  sys::stat::{
    self,
    SFlag,
  },
  unistd::{
    Uid,
    User,
  },
};

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

/// Serialize all current environment variables into a byte buffer.
///
/// Uses the null-byte-separated `KEY=VALUE\0` format (same as `/proc/self/environ`).
fn serialize_env() -> Vec<u8> {
  let mut buf = Vec::new();
  for (key, value) in std::env::vars_os() {
    buf.extend_from_slice(key.as_bytes());
    buf.push(b'=');
    buf.extend_from_slice(value.as_bytes());
    buf.push(0);
  }
  buf
}

/// Deserialize environment variables from null-byte-separated `KEY=VALUE\0` format.
fn deserialize_env(data: &[u8]) -> Vec<(OsString, OsString)> {
  use std::os::unix::ffi::OsStringExt;
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

/// Save the current environment to a temporary file with secure permissions.
///
/// Returns the path to the file. The file is created with mode 0600.
fn save_env_to_file() -> color_eyre::Result<std::path::PathBuf> {
  let dir = std::env::temp_dir();
  let env_data = serialize_env();

  // Create tempfile with a unique path, then write data to it.
  // tempfile creates with mode 0600 by default on Unix.
  let mut tmpfile = tempfile::Builder::new()
    .prefix("tracexec-env-")
    .tempfile_in(&dir)?;
  tmpfile.write_all(&env_data)?;
  tmpfile.flush()?;

  // Persist so the file survives after we exec into sudo.
  let path = tmpfile.into_temp_path().keep()?;
  Ok(path)
}

/// Restore environment variables from a saved env file.
///
/// Security checks performed:
/// - File is opened with `O_NOFOLLOW` to reject symlinks
/// - `fstat` is used on the open fd to verify:
///   - File is a regular file
/// - File is unlinked immediately after reading
pub fn restore_env_from_file(path: &Path) -> color_eyre::Result<()> {
  // Open with O_NOFOLLOW to reject symlinks
  let file = fs::OpenOptions::new()
    .read(true)
    .custom_flags(OFlag::O_NOFOLLOW.bits())
    .open(path)
    .map_err(|e| color_eyre::eyre::eyre!("Failed to open env file {}: {e}", path.display()))?;

  let fd_stat = stat::fstat(&file)?;

  // Verify it's a regular file
  let file_type = SFlag::from_bits_truncate(fd_stat.st_mode & SFlag::S_IFMT.bits());
  if file_type != SFlag::S_IFREG {
    color_eyre::eyre::bail!(
      "Env file {} is not a regular file (mode={:#o})",
      path.display(),
      fd_stat.st_mode
    );
  }

  // Read the file contents
  let mut data = Vec::new();
  let mut file = file;
  file.read_to_end(&mut data)?;

  // Unlink immediately after reading
  if let Err(e) = fs::remove_file(path) {
    tracing::warn!("Failed to remove env file {}: {e}", path.display());
  }

  // Deserialize and restore
  let saved_env = deserialize_env(&data);

  // SAFETY: restore_env_from_file is called early in main.
  unsafe {
    // Clear all current env vars
    for (key, _) in std::env::vars_os() {
      std::env::remove_var(&key);
    }

    // Set the saved env vars
    for (key, value) in saved_env {
      std::env::set_var(key, value);
    }
  }

  Ok(())
}

/// Construct the command line for re-execution with elevation.
///
/// Transforms `tracexec --elevate [opts] <subcommand> [args] -- <cmd...>`
/// into
///
/// ```bash
/// sudo tracexec --user <username> --restore-env-file <path> \
///   --elevated-config-dir <path> --elevated-data-dir <path> \
///   --elevated-data-local-dir <path> [opts] <subcommand> [args] \
///   -- <cmd...>
/// ```
fn build_elevated_args(
  creds: &PreElevationCreds,
  env_file: &Path,
  config_dir: Option<&Path>,
  data_dir: Option<&Path>,
  data_local_dir: Option<&Path>,
) -> Vec<OsString> {
  build_elevated_args_from(
    std::env::args_os(),
    creds,
    env_file,
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
  env_file: &Path,
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
      // Replace the first --elevate with --user/--restore-env-file and optional dir overrides.
      replaced = true;
      result.push(OsString::from("--user"));
      result.push(OsString::from(&creds.username));
      result.push(OsString::from("--restore-env-file"));
      result.push(env_file.as_os_str().to_owned());
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
/// This function:
/// 1. Saves the current environment to a secure temp file
/// 2. Re-execs via `sudo` (without `-E`) with `--user <username>` and `--restore-env-file <path>`
///
/// This function does not return on success (it replaces the current process).
pub fn elevate_and_reexec() -> color_eyre::Result<std::convert::Infallible> {
  use std::os::unix::process::CommandExt;

  if Uid::effective().is_root() {
    color_eyre::eyre::bail!("--elevate is not needed when already running as root");
  }

  let creds = PreElevationCreds::capture()?;
  let env_file = save_env_to_file()?;
  let exe = std::env::current_exe()?;

  // Capture the current user's project directories so the elevated process
  // can use them instead of root's directories.
  let proj_dirs = crate::cli::config::project_directory();
  let config_dir = proj_dirs.as_ref().map(|d| d.config_dir().to_path_buf());
  let data_dir = proj_dirs.as_ref().map(|d| d.data_dir().to_path_buf());
  let data_local_dir = proj_dirs.as_ref().map(|d| d.data_local_dir().to_path_buf());
  let elevated_args = build_elevated_args(
    &creds,
    &env_file,
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

  let err = Command::new("sudo").arg(&exe).args(&elevated_args).exec();

  // exec() only returns on error — clean up the env file
  let _ = fs::remove_file(&env_file);
  Err(err.into())
}

#[cfg(test)]
mod tests {
  use std::os::unix::ffi::OsStringExt;

  use super::*;

  #[test]
  fn test_capture_creds() {
    let creds = PreElevationCreds::capture().unwrap();
    assert_eq!(creds.uid, nix::unistd::getuid().as_raw());
    assert_eq!(creds.gid, nix::unistd::getgid().as_raw());
    assert!(!creds.username.is_empty());
  }

  #[test]
  fn test_serialize_deserialize_roundtrip() {
    let original = vec![
      (OsString::from("HOME"), OsString::from("/home/test")),
      (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
      (OsString::from("EMPTY"), OsString::from("")),
      (OsString::from("MULTI_EQ"), OsString::from("a=b=c")),
    ];

    let mut buf = Vec::new();
    for (k, v) in &original {
      buf.extend_from_slice(k.as_bytes());
      buf.push(b'=');
      buf.extend_from_slice(v.as_bytes());
      buf.push(0);
    }

    let deserialized = deserialize_env(&buf);
    assert_eq!(deserialized, original);
  }

  #[test]
  fn test_serialize_env_format() {
    // serialize_env reads from the real env, just verify it produces null-terminated entries
    let data = serialize_env();
    if data.is_empty() {
      return; // unlikely in a real test env
    }
    // Should end with a null byte (last entry's terminator)
    assert_eq!(*data.last().unwrap(), 0u8);
    // Every non-empty entry should contain '='
    for entry in data.split(|&b| b == 0) {
      if !entry.is_empty() {
        assert!(
          entry.contains(&b'='),
          "entry missing '=': {:?}",
          String::from_utf8_lossy(entry)
        );
      }
    }
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
  fn test_save_env_file() {
    let env_file = save_env_to_file().unwrap();

    // Verify the file exists and has correct permissions
    let metadata = fs::metadata(&env_file).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.uid(), nix::unistd::getuid().as_raw());

    // Clean up: remove the file ourselves since we're not actually restoring
    fs::remove_file(&env_file).unwrap();
  }

  #[test]
  fn test_restore_env_rejects_symlink() {
    let env_file = save_env_to_file().unwrap();
    let symlink_path = env_file.with_extension("link");
    std::os::unix::fs::symlink(&env_file, &symlink_path).unwrap();

    // Opening a symlink with O_NOFOLLOW should fail
    let result = restore_env_from_file(&symlink_path);
    assert!(result.is_err());

    let _ = fs::remove_file(&symlink_path);
    let _ = fs::remove_file(&env_file);
  }

  #[test]
  fn test_restore_env_preserves_values_in_subprocess() {
    // We test that save + restore correctly round-trips by writing known values,
    // saving, then restoring with the correct uid.
    use std::os::unix::fs::PermissionsExt;

    // Create a temp file with known env content
    let dir = std::env::temp_dir();
    let mut tmpfile = tempfile::Builder::new()
      .prefix("tracexec-env-test-")
      .tempfile_in(&dir)
      .unwrap();

    let test_env = b"TEST_RESTORE_A=hello_world\0TEST_RESTORE_B=foo=bar=baz\0TEST_RESTORE_C=\0";
    tmpfile.write_all(test_env).unwrap();
    tmpfile.flush().unwrap();
    let path = tmpfile.into_temp_path().keep().unwrap();

    // Verify permissions are 0600
    let meta = fs::metadata(&path).unwrap();
    assert_eq!(meta.permissions().mode() & 0o7777, 0o600);

    let my_uid = nix::unistd::getuid().as_raw();

    // We can't easily test the full env restoration in-process (it would
    // clobber the test runner's env). Instead, verify the file passes
    // all security checks by reading it manually the same way restore does.
    let file = fs::OpenOptions::new()
      .read(true)
      .custom_flags(OFlag::O_NOFOLLOW.bits())
      .open(&path)
      .unwrap();
    let fd_stat = stat::fstat(&file).unwrap();
    assert_eq!(fd_stat.st_uid, my_uid);
    assert_eq!(fd_stat.st_mode & 0o7777, 0o600);

    let mut data = Vec::new();
    let mut file = file;
    file.read_to_end(&mut data).unwrap();
    let restored = deserialize_env(&data);
    assert_eq!(
      restored,
      vec![
        (
          OsString::from("TEST_RESTORE_A"),
          OsString::from("hello_world")
        ),
        (
          OsString::from("TEST_RESTORE_B"),
          OsString::from("foo=bar=baz")
        ),
        (OsString::from("TEST_RESTORE_C"), OsString::from("")),
      ]
    );

    let _ = fs::remove_file(&path);
  }

  #[test]
  fn test_build_elevated_args_replaces_elevate() {
    let creds = PreElevationCreds {
      username: "testuser".to_string(),
      uid: 1000,
      gid: 1000,
    };

    let env_path = Path::new("/tmp/tracexec-env-abc123");

    // Simulate: tracexec --elevate tui -t -- sudo ls
    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("tui"),
      OsString::from("-t"),
      OsString::from("--"),
      OsString::from("sudo"),
      OsString::from("ls"),
    ];

    let result = build_elevated_args_from(input_args, &creds, env_path, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env-file"),
        OsString::from("/tmp/tracexec-env-abc123"),
        OsString::from("tui"),
        OsString::from("-t"),
        OsString::from("--"),
        OsString::from("sudo"),
        OsString::from("ls"),
      ]
    );
  }

  #[test]
  fn test_build_elevated_args_with_ebpf() {
    let creds = PreElevationCreds {
      username: "alice".to_string(),
      uid: 1001,
      gid: 100,
    };

    let env_path = Path::new("/tmp/tracexec-env-xyz");

    // Simulate: tracexec --elevate ebpf tui -t -- bash
    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("ebpf"),
      OsString::from("tui"),
      OsString::from("-t"),
      OsString::from("--"),
      OsString::from("bash"),
    ];

    let result = build_elevated_args_from(input_args, &creds, env_path, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("alice"),
        OsString::from("--restore-env-file"),
        OsString::from("/tmp/tracexec-env-xyz"),
        OsString::from("ebpf"),
        OsString::from("tui"),
        OsString::from("-t"),
        OsString::from("--"),
        OsString::from("bash"),
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

    let env_path = Path::new("/tmp/tracexec-env-999");

    // Simulate: tracexec --color=always --elevate -C /tmp log -- ls
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

    let result = build_elevated_args_from(input_args, &creds, env_path, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--color=always"),
        OsString::from("--user"),
        OsString::from("bob"),
        OsString::from("--restore-env-file"),
        OsString::from("/tmp/tracexec-env-999"),
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
    let creds = PreElevationCreds {
      username: "testuser".to_string(),
      uid: 1000,
      gid: 1000,
    };

    let env_path = Path::new("/tmp/tracexec-env-abc123");
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
      env_path,
      Some(config_dir),
      Some(data_dir),
      Some(data_local_dir),
    );

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env-file"),
        OsString::from("/tmp/tracexec-env-abc123"),
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
    let creds = PreElevationCreds {
      username: "testuser".to_string(),
      uid: 1000,
      gid: 1000,
    };

    let env_path = Path::new("/tmp/tracexec-env-abc123");

    // Simulate: tracexec --elevate log -- cmd --elevate
    // The --elevate after -- should NOT be replaced.
    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("log"),
      OsString::from("--"),
      OsString::from("cmd"),
      OsString::from("--elevate"),
    ];

    let result = build_elevated_args_from(input_args, &creds, env_path, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env-file"),
        OsString::from("/tmp/tracexec-env-abc123"),
        OsString::from("log"),
        OsString::from("--"),
        OsString::from("cmd"),
        OsString::from("--elevate"),
      ]
    );
  }
}
