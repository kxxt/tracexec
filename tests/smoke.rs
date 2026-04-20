use std::process::Command;

use assert_cmd::{
  cargo,
  prelude::*,
};
use predicates::prelude::*;
use serial_test::file_serial;

#[test]
#[file_serial]
// tracexec is a subprocess of the test runner,
// this might surprise the tracer of other tests because tracer doesn't expect other subprocesses.
fn log_mode_without_args_works() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd.arg("log").arg("--").arg("/proc/self/exe").arg("--help");
  cmd
    .assert()
    .success()
    .stderr(predicate::str::contains("/proc/self/exe"));
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn elevate_fails_when_already_root() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd.arg("--elevate").arg("log").arg("--").arg("true");
  cmd.assert().failure().stderr(predicate::str::contains(
    "not needed when already running as root",
  ));
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn elevate_log_mode_runs_tracee_as_original_user() -> Result<(), Box<dyn std::error::Error>> {
  // This test must be run as root with: sudo -E cargo test
  // It verifies that --user causes the tracee to run as the specified user.
  let username = std::env::var("SUDO_USER").unwrap_or_else(|_| "nobody".to_string());
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .arg("--user")
    .arg(&username)
    .arg("log")
    .arg("--")
    .arg("id")
    .arg("-un");
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains(&username));
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn elevate_restore_env_preserves_custom_env_var() -> Result<(), Box<dyn std::error::Error>> {
  // Test that --restore-env restores environment variables correctly.
  // Pass env entries via CLI args and verify the tracee sees them but not SUDO_USER.
  let username = std::env::var("SUDO_USER").unwrap_or_else(|_| "nobody".to_string());

  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .arg("--user")
    .arg(&username)
    .arg("--restore-env")
    .arg("TRACEXEC_TEST_MARKER=preserved_value")
    .arg("--restore-env")
    .arg("HOME=/tmp")
    .arg("--restore-env")
    .arg(format!(
      "PATH={}",
      std::env::var("PATH").unwrap_or_default()
    ))
    .arg("log")
    .arg("--")
    .arg("sh")
    .arg("-c")
    .arg("echo MARKER=$TRACEXEC_TEST_MARKER SUDO=$SUDO_USER");
  let output = cmd.output()?;
  let stdout = String::from_utf8_lossy(&output.stdout);

  assert!(
    stdout.contains("MARKER=preserved_value"),
    "Custom env var should be preserved, got stdout: {stdout}"
  );
  assert!(
    stdout.contains("SUDO=\n"),
    "SUDO_USER should be empty/absent, got stdout: {stdout}"
  );
  Ok(())
}
