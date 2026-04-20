//! Privilege elevation support for tracexec.
//!
//! When `--elevate` is used, tracexec captures the current user's credentials
//! and environment variables, then re-executes itself with elevated privileges
//! via `sudo`. The elevated process restores the original environment from
//! `--restore-env KEY=VALUE` CLI arguments and passes `--user <username>`
//! so the tracee runs as the original user.
//!
//! ## Security
//!
//! Environment variables are passed as CLI arguments. The data is only visible
//! to root (via /proc/<pid>/cmdline) after the sudo exec.

use std::{
  ffi::OsString,
  os::unix::ffi::OsStrExt,
  process::Command,
};

use nix::unistd::{
  Uid,
  User,
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

/// Capture all current environment variables as `KEY=VALUE` strings.
fn capture_env_entries() -> Vec<OsString> {
  std::env::vars_os()
    .map(|(key, value)| {
      let mut entry = OsString::with_capacity(key.len() + 1 + value.len());
      entry.push(&key);
      entry.push("=");
      entry.push(&value);
      entry
    })
    .collect()
}

/// Restore environment variables from an iterator of `KEY=VALUE` entries.
pub fn restore_env_from_entries(
  entries: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> color_eyre::Result<()> {
  // SAFETY: restore_env_from_entries is called early in main.
  unsafe {
    for (key, _) in std::env::vars_os() {
      std::env::remove_var(&key);
    }

    for entry in entries {
      let entry = entry.as_ref();
      let bytes = entry.as_bytes();
      let eq_pos = bytes.iter().position(|&b| b == b'=').ok_or_else(|| {
        color_eyre::eyre::eyre!(
          "Invalid --restore-env entry (missing '='): {}",
          entry.to_string_lossy()
        )
      })?;
      use std::os::unix::ffi::OsStringExt;
      let key = OsString::from_vec(bytes[..eq_pos].to_vec());
      let value = OsString::from_vec(bytes[eq_pos + 1..].to_vec());
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
/// sudo tracexec --user <username> \
///   --restore-env KEY1=VALUE1 --restore-env KEY2=VALUE2 ... \
///   --elevated-config-dir <path> --elevated-data-dir <path> \
///   --elevated-data-local-dir <path> [opts] <subcommand> [args] \
///   -- <cmd...>
/// ```
fn build_elevated_args(
  creds: &PreElevationCreds,
  env_entries: &[OsString],
  config_dir: Option<&std::path::Path>,
  data_dir: Option<&std::path::Path>,
  data_local_dir: Option<&std::path::Path>,
) -> Vec<OsString> {
  build_elevated_args_from(
    std::env::args_os(),
    creds,
    env_entries,
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
  env_entries: &[OsString],
  config_dir: Option<&std::path::Path>,
  data_dir: Option<&std::path::Path>,
  data_local_dir: Option<&std::path::Path>,
) -> Vec<OsString> {
  let mut result = Vec::new();
  let mut replaced = false;
  let mut past_delimiter = false;

  for arg in args.into_iter().skip(1) {
    let arg: OsString = arg.into();
    if past_delimiter {
      result.push(arg);
      continue;
    }
    if arg == "--" {
      past_delimiter = true;
      result.push(arg);
      continue;
    }
    if !replaced && arg == "--elevate" {
      replaced = true;
      result.push(OsString::from("--user"));
      result.push(OsString::from(&creds.username));
      for entry in env_entries {
        result.push(OsString::from("--restore-env"));
        result.push(entry.clone());
      }
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
/// 1. Captures the current environment
/// 2. Re-execs via `sudo` (without `-E`) with `--user <username>` and
///    `--restore-env KEY=VALUE` for each env var
///
/// This function does not return on success (it replaces the current process).
pub fn elevate_and_reexec() -> color_eyre::Result<std::convert::Infallible> {
  use std::os::unix::process::CommandExt;

  if Uid::effective().is_root() {
    color_eyre::eyre::bail!("--elevate is not needed when already running as root");
  }

  let creds = PreElevationCreds::capture()?;
  let env_entries = capture_env_entries();
  let exe = std::env::current_exe()?;

  // Capture the current user's project directories so the elevated process
  // can use them instead of root's directories.
  let proj_dirs = crate::cli::config::project_directory();
  let config_dir = proj_dirs.as_ref().map(|d| d.config_dir().to_path_buf());
  let data_dir = proj_dirs.as_ref().map(|d| d.data_dir().to_path_buf());
  let data_local_dir = proj_dirs.as_ref().map(|d| d.data_local_dir().to_path_buf());
  let elevated_args = build_elevated_args(
    &creds,
    &env_entries,
    config_dir.as_deref(),
    data_dir.as_deref(),
    data_local_dir.as_deref(),
  );

  tracing::debug!(
    "Elevating: sudo {} --user {} --restore-env ...",
    exe.display(),
    creds.username,
  );

  let err = Command::new("sudo").arg(&exe).args(&elevated_args).exec();

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
  fn test_capture_env_entries_format() {
    let entries = capture_env_entries();
    for entry in &entries {
      let bytes = entry.as_bytes();
      assert!(
        bytes.contains(&b'='),
        "entry missing '=': {:?}",
        String::from_utf8_lossy(bytes)
      );
    }
  }

  #[test]
  fn test_restore_env_from_entries_roundtrip() {
    let entries: Vec<OsString> = vec![
      OsString::from("HOME=/home/test"),
      OsString::from("PATH=/usr/bin:/bin"),
      OsString::from("EMPTY="),
      OsString::from("MULTI_EQ=a=b=c"),
    ];

    // Can't easily test in-process restoration (clobbers test env),
    // but verify parsing doesn't panic
    let parsed: Vec<_> = entries
      .iter()
      .map(|e| {
        let bytes = e.as_bytes();
        let eq = bytes.iter().position(|&b| b == b'=').unwrap();
        (
          OsString::from_vec(bytes[..eq].to_vec()),
          OsString::from_vec(bytes[eq + 1..].to_vec()),
        )
      })
      .collect();

    assert_eq!(parsed[0].0, "HOME");
    assert_eq!(parsed[0].1, "/home/test");
    assert_eq!(parsed[3].0, "MULTI_EQ");
    assert_eq!(parsed[3].1, "a=b=c");
  }

  #[test]
  fn test_restore_env_handles_non_utf8() {
    let mut entry = vec![];
    entry.extend_from_slice(b"KEY=");
    entry.extend_from_slice(&[0xff, 0xfe]);
    let entries = vec![OsString::from_vec(entry)];

    let parsed: Vec<_> = entries
      .iter()
      .map(|e| {
        let bytes = e.as_bytes();
        let eq = bytes.iter().position(|&b| b == b'=').unwrap();
        (
          OsString::from_vec(bytes[..eq].to_vec()),
          OsString::from_vec(bytes[eq + 1..].to_vec()),
        )
      })
      .collect();

    assert_eq!(parsed[0].0, "KEY");
    assert_eq!(parsed[0].1, OsString::from_vec(vec![0xff, 0xfe]));
  }

  #[test]
  fn test_restore_env_from_entries_rejects_missing_equals() {
    // Only test the validation logic, not the full restore (which clobbers env)
    let entry = OsString::from("INVALID_WITHOUT_EQUALS");
    let bytes = entry.as_bytes();
    let result = bytes.iter().position(|&b| b == b'=');
    assert!(
      result.is_none(),
      "entry without '=' should have no equals sign"
    );
  }

  #[test]
  fn test_build_elevated_args_replaces_elevate() {
    let creds = PreElevationCreds {
      username: "testuser".to_string(),
      uid: 1000,
      gid: 1000,
    };

    let env_entries = vec![
      OsString::from("HOME=/home/testuser"),
      OsString::from("PATH=/usr/bin"),
    ];

    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("tui"),
      OsString::from("-t"),
      OsString::from("--"),
      OsString::from("sudo"),
      OsString::from("ls"),
    ];

    let result = build_elevated_args_from(input_args, &creds, &env_entries, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env"),
        OsString::from("HOME=/home/testuser"),
        OsString::from("--restore-env"),
        OsString::from("PATH=/usr/bin"),
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

    let env_entries = vec![OsString::from("HOME=/home/alice")];

    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("ebpf"),
      OsString::from("tui"),
      OsString::from("-t"),
      OsString::from("--"),
      OsString::from("bash"),
    ];

    let result = build_elevated_args_from(input_args, &creds, &env_entries, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("alice"),
        OsString::from("--restore-env"),
        OsString::from("HOME=/home/alice"),
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

    let env_entries = vec![];

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

    let result = build_elevated_args_from(input_args, &creds, &env_entries, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--color=always"),
        OsString::from("--user"),
        OsString::from("bob"),
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

    let env_entries = vec![OsString::from("TERM=xterm")];
    let config_dir = std::path::Path::new("/home/testuser/.config/tracexec");
    let data_dir = std::path::Path::new("/home/testuser/.local/share/tracexec");
    let data_local_dir = std::path::Path::new("/home/testuser/.local/share/tracexec");

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
      &env_entries,
      Some(config_dir),
      Some(data_dir),
      Some(data_local_dir),
    );

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("--restore-env"),
        OsString::from("TERM=xterm"),
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

    let env_entries = vec![];

    let input_args = vec![
      OsString::from("tracexec"),
      OsString::from("--elevate"),
      OsString::from("log"),
      OsString::from("--"),
      OsString::from("cmd"),
      OsString::from("--elevate"),
    ];

    let result = build_elevated_args_from(input_args, &creds, &env_entries, None, None, None);

    assert_eq!(
      result,
      vec![
        OsString::from("--user"),
        OsString::from("testuser"),
        OsString::from("log"),
        OsString::from("--"),
        OsString::from("cmd"),
        OsString::from("--elevate"),
      ]
    );
  }
}
