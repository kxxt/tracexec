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
#[file_serial]
fn generate_completions_runs_from_binary() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd.arg("generate-completions").arg("bash");
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("_tracexec"));
  Ok(())
}

#[test]
#[file_serial]
fn ebpf_tui_tty_without_command_errors_before_tracing() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .arg("--color")
    .arg("never")
    .arg("ebpf")
    .arg("tui")
    .arg("--tty");
  cmd.assert().failure().stderr(predicate::str::contains(
    "not supported for eBPF system-wide tracing",
  ));
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
fn restore_env_socket_passes_full_env_to_tracee() -> Result<(), Box<dyn std::error::Error>> {
  use std::{
    io::{
      Read,
      Write,
    },
    os::{
      linux::net::SocketAddrExt,
      unix::net::UnixListener,
    },
    thread,
    time::{
      Duration,
      SystemTime,
      UNIX_EPOCH,
    },
  };

  let username = std::env::var("SUDO_USER").unwrap_or_else(|_| "nobody".to_string());
  let socket_name = format!(
    "tracexec-test-env-{}-{}",
    std::process::id(),
    SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
  );
  let listener = UnixListener::bind_addr(&std::os::unix::net::SocketAddr::from_abstract_name(
    socket_name.as_bytes(),
  )?)?;

  let mut env_data = Vec::new();
  env_data.extend_from_slice(b"TRACEXEC_TEST_MARKER=preserved_value\0");
  env_data.extend_from_slice(b"SUDO_USER=from_socket\0");
  env_data.extend_from_slice(b"HOME=/tmp\0");
  env_data
    .extend_from_slice(format!("PATH={}\0", std::env::var("PATH").unwrap_or_default()).as_bytes());

  let server = thread::spawn(move || -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let (mut stream, _) = loop {
      match listener.accept() {
        Ok(accepted) => break accepted,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
          if std::time::Instant::now() >= deadline {
            return Err(std::io::Error::new(
              std::io::ErrorKind::TimedOut,
              "timed out waiting for env request",
            ));
          }
          thread::sleep(Duration::from_millis(10));
        }
        Err(e) => return Err(e),
      }
    };
    let mut request = [0; 15];
    stream.read_exact(&mut request)?;
    assert_eq!(&request, b"tracexec-env-v1");
    stream.write_all(&(env_data.len() as u32).to_be_bytes())?;
    stream.write_all(&env_data)?;
    stream.flush()
  });

  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .arg("--user")
    .arg(&username)
    .arg("--restore-env-socket")
    .arg(&socket_name)
    .arg("log")
    .arg("--")
    .arg("sh")
    .arg("-c")
    .arg("echo MARKER=$TRACEXEC_TEST_MARKER SUDO=$SUDO_USER");
  let output = cmd.output()?;
  server.join().expect("env server panicked")?;
  let stdout = String::from_utf8_lossy(&output.stdout);

  assert!(
    stdout.contains("MARKER=preserved_value"),
    "Custom env var should be preserved, got stdout: {stdout}"
  );
  assert!(
    stdout.contains("SUDO=from_socket\n"),
    "SUDO_USER should come from the transferred original env, got stdout: {stdout}"
  );
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn ebpf_log_mode_runs_tracee() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .env("TRACEXEC_USE_KPROBE", "1")
    .arg("--color")
    .arg("never")
    .arg("ebpf")
    .arg("log")
    .arg("-o")
    .arg("-")
    .arg("--")
    .arg("sh")
    .arg("-c")
    .arg("true");
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("sh"));
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn ebpf_collect_json_stream_runs_tracee() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .env("TRACEXEC_USE_KPROBE", "1")
    .arg("--color")
    .arg("never")
    .arg("ebpf")
    .arg("collect")
    .arg("-F")
    .arg("json-stream")
    .arg("-o")
    .arg("-")
    .arg("--")
    .arg("sh")
    .arg("-c")
    .arg("true");
  cmd.assert().success().stdout(
    predicate::str::contains("\"generator\":\"tracexec_exporter_json\"")
      .and(predicate::str::contains("\"syscall\":\"execve\""))
      .and(predicate::str::contains("\"sh\"")),
  );
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn ebpf_collect_json_runs_tracee() -> Result<(), Box<dyn std::error::Error>> {
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .env("TRACEXEC_USE_KPROBE", "1")
    .arg("--color")
    .arg("never")
    .arg("ebpf")
    .arg("collect")
    .arg("-F")
    .arg("json")
    .arg("-o")
    .arg("-")
    .arg("--")
    .arg("sh")
    .arg("-c")
    .arg("true");
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("\"events\""));
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn ebpf_collect_perfetto_runs_tracee() -> Result<(), Box<dyn std::error::Error>> {
  let output = std::env::temp_dir().join(format!(
    "tracexec-ebpf-collect-perfetto-{}.pftrace",
    std::process::id()
  ));
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .env("TRACEXEC_USE_KPROBE", "1")
    .arg("--color")
    .arg("never")
    .arg("ebpf")
    .arg("collect")
    .arg("-F")
    .arg("perfetto")
    .arg("-o")
    .arg(&output)
    .arg("--")
    .arg("sh")
    .arg("-c")
    .arg("true");
  cmd.assert().success();
  assert!(std::fs::metadata(&output)?.len() > 0);
  let _ = std::fs::remove_file(output);
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn ebpf_log_handles_malformed_exec_fixtures() -> Result<(), Box<dyn std::error::Error>> {
  for fixture in [
    env!("CARGO_BIN_EXE_corrupted-envp"),
    env!("CARGO_BIN_EXE_empty-argv"),
  ] {
    let mut cmd = Command::new(cargo::cargo_bin!());
    cmd
      .env("TRACEXEC_USE_KPROBE", "1")
      .arg("--color")
      .arg("never")
      .arg("ebpf")
      .arg("log")
      .arg("-o")
      .arg("-")
      .arg("--")
      .arg(fixture);
    cmd.assert().success();
  }
  Ok(())
}

#[test]
#[file_serial(ignored)]
#[ignore = "root"]
fn ebpf_log_handles_threaded_and_repeated_exec_fixtures() -> Result<(), Box<dyn std::error::Error>>
{
  let mut cmd = Command::new(cargo::cargo_bin!());
  cmd
    .env("TRACEXEC_USE_KPROBE", "1")
    .arg("--color")
    .arg("never")
    .arg("ebpf")
    .arg("log")
    .arg("-o")
    .arg("-")
    .arg("--")
    .arg(env!("CARGO_BIN_EXE_exec-stress"))
    .arg("2");
  cmd.assert().success();
  Ok(())
}
