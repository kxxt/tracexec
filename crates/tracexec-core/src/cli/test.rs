// Disabled. TODO
use assert_cmd::prelude::*;
use predicates::prelude::*;
use serial_test::file_serial;
use std::process::Command;

#[test]
#[file_serial]
// tracexec is a subprocess of the test runner,
// this might surprise the tracer of other tests because tracer doesn't expect other subprocesses.
fn log_mode_without_args_works() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::cargo_bin("tracexec")?;
  cmd.arg("log").arg("--").arg("/proc/self/exe").arg("--help");
  cmd
    .assert()
    .success()
    .stderr(predicate::str::contains("/proc/self/exe"));
  Ok(())
}
