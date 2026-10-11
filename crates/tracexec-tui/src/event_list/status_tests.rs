use std::{
  cell::RefCell,
  rc::Rc,
  sync::Arc,
};

use chrono::TimeDelta;
use nix::{
  errno::Errno,
  unistd::Pid,
};
use tracexec_core::{
  breakpoint::{
    BreakPointHit,
    BreakPointStop,
  },
  cli::args::ModifierArgs,
  event::{
    EventId,
    EventStatus,
    ProcessStateUpdate,
    ProcessStateUpdateEvent,
    TracerEventDetails,
  },
  timestamp::ts_from_boot_ns,
  tracer::ProcessExit,
};

use super::{
  Event,
  EventList,
};
use crate::{
  event::tests::{
    baseline,
    exec_details,
  },
  event_line::EventLine,
  theme::current_theme,
};

fn event_list(timestamp: bool) -> EventList {
  EventList::new(
    Arc::new(baseline()),
    false,
    ModifierArgs {
      timestamp,
      inline_timestamp_format: Some(Default::default()),
      stdio_in_cmdline: true,
      fd_in_cmdline: true,
      ..Default::default()
    },
    100,
    true,
    false,
    true,
    current_theme(),
  )
}

fn assert_matches_rebuilt(list: &EventList, id: EventId) {
  let (storage, event) = &list.event_map[&id];
  let fresh = event.borrow().to_event_line(
    &list.baseline,
    &list.event_modifier(),
    storage.extra_prefix.clone(),
    list.theme,
  );
  assert_eq!(storage.line.line, fresh.line);
  for (cached, rebuilt) in [
    (&storage.line.cwd_mask, fresh.cwd_mask),
    (&storage.line.env_mask, fresh.env_mask),
  ] {
    assert_eq!(
      cached.as_ref().map(|mask| (&mask.range, &mask.values)),
      rebuilt.as_ref().map(|mask| (&mask.range, &mask.values))
    );
  }
}

fn content_pointers(line: &EventLine) -> Vec<*const u8> {
  line
    .line
    .spans
    .iter()
    .enumerate()
    .filter(|(index, _)| Some(*index) != line.status_span)
    .map(|(_, span)| span.content.as_ptr())
    .collect()
}

fn update_and_check_reuse(list: &mut EventList, id: EventId, update: ProcessStateUpdate) {
  let line = &list.event_map[&id].0.line;
  let spans = line.line.spans.as_ptr();
  let contents = content_pointers(line);
  list.should_refresh_list_cache = false;
  list.update(ProcessStateUpdateEvent {
    ids: vec![id],
    pid: Pid::from_raw(42),
    update,
  });
  let line = &list.event_map[&id].0.line;
  // A full rebuild allocates replacement spans while the old line is still
  // alive. Retaining both the vector and its strings guards against that cost.
  assert_eq!(line.line.spans.as_ptr(), spans);
  assert_eq!(content_pointers(line), contents);
  assert!(list.should_refresh_list_cache);
  assert_matches_rebuilt(list, id);
}

#[test]
fn status_updates_reuse_commandline_spans_with_display_toggles() {
  let id = EventId::zero();
  let hit = BreakPointHit {
    bid: 1,
    pid: Pid::from_raw(42),
    stop: BreakPointStop::SyscallExit,
  };
  for timestamp in [false, true] {
    let mut list = event_list(timestamp);
    list.push(id, exec_details());
    list.set_window((0, 1));
    for update in [
      ProcessStateUpdate::BreakPointHit(hit),
      ProcessStateUpdate::Resumed,
      ProcessStateUpdate::ResumeError {
        hit,
        error: Errno::ESRCH,
      },
      ProcessStateUpdate::Exit {
        status: ProcessExit::Code(0),
        timestamp: ts_from_boot_ns(1010),
      },
    ] {
      list.toggle_env_display();
      update_and_check_reuse(&mut list, id, update);
      list.toggle_cwd_display();
      assert_matches_rebuilt(&list, id);
    }
    assert_eq!(
      list.event_map[&id].1.borrow().elapsed,
      Some(TimeDelta::nanoseconds(1000))
    );
    // Rebuilding for changed formatting must install a new, valid status slot.
    list.modifier_args.timestamp = !timestamp;
    list.rebuild_lines();
    update_and_check_reuse(
      &mut list,
      id,
      ProcessStateUpdate::Detached {
        hid: 1,
        timestamp: ts_from_boot_ns(2010),
      },
    );
    assert_eq!(
      list.event_map[&id].1.borrow().elapsed,
      Some(TimeDelta::nanoseconds(2000))
    );
  }
}

#[test]
fn prefixed_lines_can_gain_and_clear_status_without_shifting_masks() {
  let id = EventId::zero();
  for details in [
    exec_details(),
    TracerEventDetails::NewChild {
      ppid: Pid::from_raw(1),
      pcomm: "parent".into(),
      pid: Pid::from_raw(42),
      timestamp: ts_from_boot_ns(10),
    },
  ] {
    let mut list = event_list(true);
    list.dumb_push(
      Rc::new(RefCell::new(Event {
        details: Arc::new(details),
        id,
        status: None,
        elapsed: None,
      })),
      Some("parent: ".into()),
    );
    list.set_window((0, 1));
    list.toggle_env_display();
    list.toggle_cwd_display();
    update_and_check_reuse(&mut list, id, ProcessStateUpdate::Resumed);
    let (storage, event) = list.event_map.get_mut(&id).unwrap();
    let spans = storage.line.line.spans.as_ptr();
    event.borrow_mut().status = None;
    storage.line.update_status(None);
    assert_eq!(storage.line.line.spans.as_ptr(), spans);
    assert_matches_rebuilt(&list, id);
  }
}

#[test]
fn failed_exec_status_and_elapsed_are_not_overwritten() {
  let id = EventId::zero();
  for result in [-i64::from(nix::libc::ENOENT), -i64::from(nix::libc::EACCES)] {
    let mut details = exec_details();
    if let TracerEventDetails::Exec(exec) = &mut details {
      exec.result = result;
    }
    let mut list = event_list(false);
    list.push(id, details);
    let previous = list.event_map[&id].0.line.line.clone();
    list.update(ProcessStateUpdateEvent {
      ids: vec![id],
      pid: Pid::from_raw(42),
      update: ProcessStateUpdate::Exit {
        status: ProcessExit::Code(0),
        timestamp: ts_from_boot_ns(1010),
      },
    });
    let (storage, event) = &list.event_map[&id];
    assert_eq!(storage.line.line, previous);
    assert!(matches!(
      event.borrow().status,
      Some(EventStatus::ExecENOENT | EventStatus::ExecFailure)
    ));
    assert_eq!(event.borrow().elapsed, None);
  }
}
