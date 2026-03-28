use std::{
  cell::RefCell,
  collections::VecDeque,
  rc::Rc,
  sync::LazyLock,
};

use crossterm::event::KeyEvent;
use ratatui::{
  buffer::Buffer,
  layout::{
    Alignment,
    Rect,
  },
  style::Styled,
  text::Line,
  widgets::{
    Block,
    Borders,
    Clear,
    StatefulWidgetRef,
    Widget,
  },
};
use tracexec_core::{
  cli::keys::TuiKeyBindings,
  event::{
    ParentEventId,
    TracerEventDetails,
  },
  primitives::local_chan::LocalUnboundedSender,
};
use tracing::debug;

use super::{
  event_list::{
    Event,
    EventList,
  },
  theme::THEME,
};
use crate::action::Action;

pub struct BacktracePopup;

#[derive(Debug)]
pub struct BacktracePopupState {
  pub(super) list: EventList,
  /// Whether there are dead events no longer in memory or not
  event_loss: bool,
  should_resize: bool,
}

type ParentAndEventQueue = VecDeque<(Option<ParentEventId>, Rc<RefCell<Event>>)>;

impl BacktracePopupState {
  pub fn new(event: Rc<RefCell<Event>>, old_list: &EventList) -> Self {
    let (trace, event_loss) = Self::collect_backtrace(event, old_list);
    let mut list = EventList::new(
      old_list.baseline.clone(),
      false,
      old_list.modifier_args.clone(),
      u64::MAX,
      false,
      old_list.has_clipboard,
      false,
    );
    list.rt_modifier = old_list.rt_modifier;
    for (p, e) in trace {
      list.dumb_push(
        e,
        match p {
          Some(ParentEventId::Become(_)) => Some(THEME.backtrace_parent_becomes.clone()),
          Some(ParentEventId::Spawn(_)) => Some(THEME.backtrace_parent_spawns.clone()),
          None => Some(THEME.backtrace_parent_unknown.clone()),
        },
      );
    }
    Self {
      list,
      event_loss,
      should_resize: true,
    }
  }

  /// Collect the backtrace and whether
  fn collect_backtrace(event: Rc<RefCell<Event>>, list: &EventList) -> (ParentAndEventQueue, bool) {
    let mut trace = VecDeque::new();
    let mut event = event;
    let event_loss = loop {
      let e = event.borrow();
      let TracerEventDetails::Exec(exec) = e.details.as_ref() else {
        panic!("back trace should only contain exec event")
      };
      let parent = exec.parent;
      drop(e);
      debug!("backtracing -- {event:?}");
      trace.push_front((parent, event));
      if let Some(parent) = parent {
        let eid = parent.into();
        if let Some(e) = list.get(eid) {
          event = e;
        } else {
          break true;
        }
      } else {
        break false;
      }
    };
    (trace, event_loss)
  }
}

static HELP: LazyLock<Line<'static>> = LazyLock::new(|| {
  Line::from(vec![
    "Legend: ".into(),
    THEME.backtrace_parent_becomes.clone(),
    " Becomes ".set_style(THEME.cli_flag),
    THEME.backtrace_parent_spawns.clone(),
    " Spawns ".set_style(THEME.cli_flag),
  ])
});

impl StatefulWidgetRef for BacktracePopup {
  type State = BacktracePopupState;

  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    Clear.render(area, buf);
    let screen = buf.area;
    let help_width = HELP.width() as u16;
    let start = screen.right().saturating_sub(help_width);
    (&*HELP).render(Rect::new(start, 0, help_width, 1), buf);
    let block = Block::new()
      .title(if !state.event_loss {
        " Backtrace "
      } else {
        " Backtrace (incomplete) "
      })
      .borders(Borders::TOP | Borders::BOTTOM)
      .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    block.render(area, buf);
    if state.should_resize {
      state.should_resize = false;
      state.list.max_window_len = inner.height as usize - 2;
      state.list.set_window((
        state.list.get_window().0,
        state.list.get_window().0 + state.list.max_window_len,
      ));
    }
    state.list.render(inner, buf);
  }
}

impl BacktracePopupState {
  pub async fn handle_key_event(
    &self,
    ke: KeyEvent,
    keys: &TuiKeyBindings,
    action_tx: &LocalUnboundedSender<Action>,
  ) -> color_eyre::Result<()> {
    if keys.close_popup.matches(ke) {
      action_tx.send(Action::CancelCurrentPopup)
    } else {
      self.list.handle_key_event(ke, keys, action_tx).await?
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    sync::Arc,
  };

  use chrono::Local;
  use insta::assert_snapshot;
  use nix::{
    errno::Errno,
    unistd::Pid,
  };
  use tracexec_core::{
    cache::ArcStr,
    cli::args::ModifierArgs,
    event::{
      EventId,
      ExecEvent,
      ExecSyscall,
      OutputMsg,
      ParentEventId,
      TracerEventDetails,
    },
    proc::{
      BaselineInfo,
      Cred,
      FileDescriptorInfoCollection,
    },
  };

  use super::{
    BacktracePopup,
    BacktracePopupState,
  };
  use crate::{
    event_list::EventList,
    test_utils::{
      test_area_full,
      test_render_stateful_widget_area,
    },
  };

  fn baseline_for_tests() -> Arc<BaselineInfo> {
    Arc::new(BaselineInfo {
      cwd: OutputMsg::Ok("cwd".into()),
      env: BTreeMap::new(),
      fdinfo: FileDescriptorInfoCollection::new_baseline().unwrap(),
    })
  }

  fn exec_event(pid: i32, parent: Option<ParentEventId>) -> ExecEvent {
    ExecEvent {
      syscall: ExecSyscall::Execve,
      from_non_main_thread: false,
      pid: Pid::from_raw(pid),
      cwd: OutputMsg::Ok("cwd".into()),
      comm: ArcStr::from("comm"),
      filename: OutputMsg::Ok("/bin/echo".into()),
      argv: Arc::new(Ok(vec![OutputMsg::Ok("echo".into())])),
      envp: Arc::new(Ok(BTreeMap::new())),
      has_dash_env: false,
      cred: Ok(Cred::default()),
      interpreter: None,
      env_diff: Err(Errno::EPERM),
      fdinfo: Arc::new(FileDescriptorInfoCollection::default()),
      result: 0,
      timestamp: Local::now(),
      parent,
    }
  }

  #[test]
  fn snapshot_backtrace_popup() {
    let mut list = EventList::new(
      baseline_for_tests(),
      false,
      ModifierArgs::default(),
      1024,
      false,
      false,
      true,
    );
    let root_id = EventId::new(1);
    let child_id = EventId::new(2);
    list.push(
      root_id,
      TracerEventDetails::Exec(Box::new(exec_event(1001, None))),
    );
    list.push(
      child_id,
      TracerEventDetails::Exec(Box::new(exec_event(
        1002,
        Some(ParentEventId::Spawn(root_id)),
      ))),
    );
    let event = list.get_for_test(child_id).unwrap();
    let mut state = BacktracePopupState::new(event, &list);
    let area = test_area_full(80, 12);
    let rendered = test_render_stateful_widget_area(BacktracePopup, area, &mut state);
    assert_snapshot!(rendered);
  }
}
