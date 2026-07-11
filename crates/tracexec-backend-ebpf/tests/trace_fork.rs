use std::{
  path::{
    Path,
    PathBuf,
  },
  process::Command,
  time::Duration,
};

use libbpf_rs::RingBufferBuilder;
use rstest::{
  fixture,
  rstest,
};
use serial_test::file_serial;
use tracexec_backend_ebpf::{
  bpf::skel::types::{
    event_type,
    fork_event,
  },
  function_name,
  test_utils::{
    find_sh,
    prepare_trace_fork_only,
    with_skel,
  },
};

mod common;

use common::EventSlot;

#[fixture]
fn sh_executable() -> PathBuf {
  find_sh()
}

struct ForkCapture {
  child_pid: i32,
  event: fork_event,
}

fn run_fork_and_capture(
  skel: &tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  sh_executable: &Path,
  timeout: Duration,
) -> color_eyre::Result<ForkCapture> {
  let event_slot = EventSlot::<fork_event>::new();

  let mut rb_builder = RingBufferBuilder::new();
  let slot = event_slot.clone();
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg("sleep 0.2 & wait")
    .spawn()?;
  let parent_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    slot.store_matching(data, |evt| evt.parent_tgid == parent_pid);
    0
  })?;
  let rb = rb_builder.build()?;

  let _status = child.wait()?;

  let event = event_slot.wait(&rb, timeout, "missing fork event for child")?;
  Ok(ForkCapture {
    child_pid: event.header.pid,
    event,
  })
}

#[rstest]
#[file_serial(bpf)]
#[ignore = "root"]
fn test_trace_fork_emits_fork_event(sh_executable: PathBuf) -> color_eyre::Result<()> {
  with_skel(function_name!(), prepare_trace_fork_only, |skel| {
    let capture = run_fork_and_capture(skel, &sh_executable, Duration::from_secs(2))?;
    assert_eq!(capture.event.header.r#type, event_type::FORK_EVENT);
    assert_ne!(capture.child_pid, capture.event.parent_tgid);
    Ok(())
  })
}
