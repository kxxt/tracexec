use std::{
  env,
  mem::MaybeUninit,
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

use libbpf_rs::{
  RingBufferBuilder,
  skel::{
    OpenSkel,
    Skel,
    SkelBuilder,
  },
};
use rstest::{
  fixture,
  rstest,
};
use tracexec_backend_ebpf::bpf::skel::{
  TracexecSystemSkelBuilder,
  types::{
    event_type,
    exit_event,
  },
};

#[fixture]
fn sh_executable() -> PathBuf {
  env::var_os("PATH")
    .and_then(|paths| {
      env::split_paths(&paths)
        .filter_map(|dir| {
          let full_path = dir.join("sh");
          if full_path.is_file() {
            Some(full_path)
          } else {
            None
          }
        })
        .next()
    })
    .expect("executable `sh` not found")
}

#[rstest]
#[ignore = "root"]
fn test_handle_exit_emits_exit_event(sh_executable: PathBuf) -> color_eyre::Result<()> {
  let mut obj = MaybeUninit::uninit();
  let builder = TracexecSystemSkelBuilder::default();
  let mut open_skel = builder.open(&mut obj)?;

  for mut prog in open_skel.open_object_mut().progs_mut() {
    prog.set_autoload(false)
  }

  open_skel.progs.handle_exit.set_autoload(true);

  if let Some(rodata) = open_skel.maps.rodata_data.as_deref_mut() {
    rodata.tracexec_config.follow_fork = MaybeUninit::new(false);
  }

  let mut skel = open_skel.load()?;
  skel.attach()?;

  let event_slot: Arc<Mutex<Option<exit_event>>> = Arc::new(Mutex::new(None));

  let mut rb_builder = RingBufferBuilder::new();
  let slot = Arc::clone(&event_slot);
  let mut child = Command::new(sh_executable)
    .arg("-c")
    .arg("exit 7")
    .spawn()?;
  let child_pid = child.id() as i32;

  rb_builder.add(&skel.maps.events, move |data| {
    if data.len() == std::mem::size_of::<exit_event>() {
      // SAFETY: exit_event is a plain old data struct produced by the eBPF program.
      let evt: &exit_event = unsafe { &*(data.as_ptr() as *const _) };
      if evt.header.pid == child_pid {
        *slot.lock().unwrap() = Some(*evt);
      }
    }
    0
  })?;
  let rb = rb_builder.build()?;

  let _status = child.wait()?;

  let start = Instant::now();
  while start.elapsed() < Duration::from_secs(2) {
    rb.poll(Duration::from_millis(50))?;
    if event_slot.lock().unwrap().is_some() {
      break;
    }
  }

  let evt = event_slot
    .lock()
    .unwrap()
    .expect("missing exit event for child");
  assert_eq!(evt.header.r#type, event_type::EXIT_EVENT);
  assert_eq!(evt.header.pid, child_pid);
  assert_eq!(evt.code, 7);
  assert_eq!(evt.sig, 0);
  Ok(())
}
