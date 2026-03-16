use std::{
  path::PathBuf,
  process::Command,
  sync::{
    Arc,
    Mutex,
  },
  time::{
    Duration,
    Instant,
  },
};

use libbpf_rs::RingBufferBuilder;
use rstest::{
  fixture,
  rstest,
};
use tracexec_backend_ebpf::bpf::skel::types::{
  event_type,
  fork_event,
};

mod bpf_test_utils;

use bpf_test_utils::{
  find_sh,
  prepare_trace_fork_only,
  with_skel,
};

#[fixture]
fn sh_executable() -> PathBuf {
  find_sh()
}

struct ForkCapture {
  child_pid: i32,
  event: fork_event,
}

fn run_fork_and_capture(
  skel: &mut tracexec_backend_ebpf::bpf::skel::TracexecSystemSkel<'_>,
  sh_executable: &PathBuf,
  timeout: Duration,
) -> color_eyre::Result<ForkCapture> {
  let event_slot: Arc<Mutex<Option<fork_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg("sleep 0.2 & wait")
    .spawn()?;
  let parent_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<fork_event>() {
      // SAFETY: fork_event is a plain old data struct produced by the eBPF program.
      let evt = unsafe { std::ptr::read(data.as_ptr() as *const fork_event) };
      if evt.parent_tgid == parent_pid {
        *slot.lock().unwrap() = Some(evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let _status = child.wait()?;

  let start = Instant::now();
  while start.elapsed() < timeout {
    rb.poll(Duration::from_millis(50))?;
    if event_slot.lock().unwrap().is_some() {
      break;
    }
  }

  let event = event_slot
    .lock()
    .unwrap()
    .expect("missing fork event for child");
  Ok(ForkCapture {
    child_pid: event.header.pid,
    event,
  })
}

#[rstest]
#[ignore = "root"]
fn test_trace_fork_emits_fork_event(sh_executable: PathBuf) -> color_eyre::Result<()> {
  with_skel(prepare_trace_fork_only, |skel| {
    let capture = run_fork_and_capture(skel, &sh_executable, Duration::from_secs(2))?;
    assert_eq!(capture.event.header.r#type, event_type::FORK_EVENT);
    assert_ne!(capture.child_pid, capture.event.parent_tgid);
    Ok(())
  })
}
