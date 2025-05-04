use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent};

use ratatui::{
  buffer::Buffer,
  layout::{Alignment::Center, Rect},
  widgets::{Block, Borders, Clear, StatefulWidgetRef, Widget},
};
use tracing::debug;

use crate::{
  action::Action, event::TracerEventDetails, primitives::local_chan::LocalUnboundedSender,
};

use super::event_list::{Event, EventList};

pub struct BacktracePopup;

#[derive(Debug)]
pub struct BacktracePopupState {
  list: EventList,
  /// Whether there are dead events no longer in memory or not
  event_loss: bool,
  should_resize: bool,
}

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
    );
    for e in trace {
      list.dumb_push(e);
    }
    Self {
      list,
      event_loss,
      should_resize: true,
    }
  }

  /// Collect the backtrace and whether
  fn collect_backtrace(
    event: Rc<RefCell<Event>>,
    list: &EventList,
  ) -> (VecDeque<Rc<RefCell<Event>>>, bool) {
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
      trace.push_front(event);
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

impl StatefulWidgetRef for BacktracePopup {
  type State = BacktracePopupState;

  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    Clear.render(area, buf);
    let block = Block::new()
      .title(if !state.event_loss {
        " Backtrace "
      } else {
        " Backtrace (incomplete) "
      })
      .borders(Borders::TOP | Borders::BOTTOM)
      .title_alignment(Center);
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
    action_tx: &LocalUnboundedSender<Action>,
  ) -> color_eyre::Result<()> {
    if ke.code == KeyCode::Char('q') {
      action_tx.send(Action::CancelCurrentPopup)
    } else {
      self.list.handle_key_event(ke, action_tx).await?
    }
    Ok(())
  }
}
