use std::{
  collections::{BTreeMap, HashMap},
  process::Stdio,
  sync::Arc,
};

use color_eyre::Section;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use itertools::Itertools;
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
  layout::{Alignment, Constraint, Layout},
  prelude::{Buffer, Rect},
  style::{Modifier, Style, Stylize},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};
use tracing::{debug, trace};
use tui_widget_list::PreRender;

use crate::{
  action::Action,
  tracer::{state::BreakPointStop, Tracer},
};

use super::{
  help::{cli_flag, help_item},
  theme::THEME,
};

#[derive(Debug, Clone)]
struct BreakPointHitEntry {
  bid: u32,
  pid: Pid,
  stop: BreakPointStop,
  selected: bool,
}

impl BreakPointHitEntry {
  fn paragraph(&self) -> Paragraph {
    let space = Span::from(" ");
    let line = Line::default()
      .spans(vec![
        Span::styled(self.pid.to_string(), THEME.hit_entry_pid),
        space.clone(),
        Span::styled("hit", THEME.hit_entry_plain_text),
        space.clone(),
        Span::styled(
          format!("breakpoint #{}", self.bid),
          THEME.hit_entry_plain_text,
        ),
        space.clone(),
        Span::styled("at", THEME.hit_entry_plain_text),
        space.clone(),
        Span::styled(
          <&'static str>::from(self.stop),
          THEME.hit_entry_breakpoint_stop,
        ),
      ])
      .style(if self.selected {
        Style::default().add_modifier(Modifier::REVERSED)
      } else {
        Style::default()
      });
    Paragraph::new(line).wrap(Wrap { trim: true })
  }
}

impl Widget for BreakPointHitEntry {
  fn render(self, area: Rect, buf: &mut Buffer) {
    self.paragraph().render(area, buf);
  }
}

impl PreRender for BreakPointHitEntry {
  fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
    self.selected = context.is_selected;
    self
      .paragraph()
      .line_count(context.cross_axis_size)
      .try_into()
      .unwrap_or(u16::MAX)
  }
}

#[derive(Debug, Clone)]
pub enum DetachReaction {
  LaunchExternal(String),
}

pub struct HitManagerState {
  tracer: Arc<Tracer>,
  counter: u64,
  hits: BTreeMap<u64, BreakPointHitEntry>,
  pending_detach_reactions: HashMap<u64, DetachReaction>,
  list_state: tui_widget_list::ListState,
  pub visible: bool,
}

impl HitManagerState {
  pub fn new(tracer: Arc<Tracer>) -> color_eyre::Result<Self> {
    Ok(Self {
      tracer,
      counter: 0,
      hits: BTreeMap::new(),
      pending_detach_reactions: HashMap::new(),
      list_state: tui_widget_list::ListState::default(),
      visible: false,
    })
  }

  pub fn count(&self) -> usize {
    self.hits.len()
  }

  pub fn help(&self) -> impl Iterator<Item = Span> {
    [
      help_item!("Q", "Back"),
      help_item!("R", "Resume\u{00a0}Process"),
      help_item!("D", "Detach\u{00a0}Process"),
      help_item!(
        "Enter",
        "Detach,\u{00a0}Stop\u{00a0}and\u{00a0}Run\u{00a0}Command"
      ),
    ]
    .into_iter()
    .flatten()
  }

  fn close_when_empty(&self) -> Option<Action> {
    if self.hits.is_empty() {
      Some(Action::HideHitManager)
    } else {
      None
    }
  }

  pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
    if key.modifiers == KeyModifiers::NONE {
      match key.code {
        KeyCode::Char('q') => return Some(Action::HideHitManager),
        KeyCode::Down | KeyCode::Char('j') => {
          self.list_state.next();
        }
        KeyCode::Up | KeyCode::Char('k') => {
          self.list_state.previous();
        }
        KeyCode::Char('d') => {
          if let Some(selected) = self.list_state.selected {
            self.select_near_by(selected);
            let hid = *self.hits.keys().nth(selected).unwrap();
            if let Err(e) = self.detach(hid) {
              return Some(Action::show_error_popup("Detach failed".to_string(), e));
            };
            return self.close_when_empty();
          }
        }
        KeyCode::Enter => {
          if let Some(selected) = self.list_state.selected {
            self.select_near_by(selected);
            let hid = *self.hits.keys().nth(selected).unwrap();
            if let Err(e) = self
              .detach_pause_and_launch_external(hid, "konsole --hold -e gdb -p {{PID}}".to_owned())
            {
              return Some(Action::show_error_popup(
                "Error".to_string(),
                e.with_note(|| "Failed to detach or launch external command"),
              ));
            }
            return self.close_when_empty();
          }
        }
        KeyCode::Char('r') => {
          if let Some(selected) = self.list_state.selected {
            debug!("selected: {}", selected);
            self.select_near_by(selected);
            let hid = *self.hits.keys().nth(selected).unwrap();
            if let Err(e) = self.resume(hid) {
              return Some(Action::show_error_popup("Resume failed".to_string(), e));
            }
            return self.close_when_empty();
          }
        }
        _ => {}
      }
    }
    None
  }

  pub fn add_hit(&mut self, bid: u32, pid: Pid, stop: BreakPointStop) -> u64 {
    let id = self.counter;
    self.hits.insert(
      id,
      BreakPointHitEntry {
        bid,
        pid,
        stop,
        selected: false,
      },
    );
    self.counter += 1;
    id
  }

  fn select_near_by(&mut self, old: usize) {
    if old > 0 {
      self.list_state.select(Some(old - 1));
    } else if old + 1 < self.hits.len() {
      self.list_state.select(Some(old + 1));
    } else {
      self.list_state.select(None);
    }
  }

  pub fn detach(&mut self, hid: u64) -> color_eyre::Result<()> {
    if let Some(hit) = self.hits.remove(&hid) {
      self.tracer.request_process_detach(hit.pid, None, hid)?;
    }
    Ok(())
  }

  pub fn resume(&mut self, hid: u64) -> color_eyre::Result<()> {
    if let Some(hit) = self.hits.remove(&hid) {
      self.tracer.request_process_resume(hit.pid, hit.stop)?;
    }
    Ok(())
  }

  pub fn detach_pause_and_launch_external(
    &mut self,
    hid: u64,
    cmdline_template: String,
  ) -> color_eyre::Result<()> {
    trace!("detaching, pausing and launching external command for hit={hid}");
    if let Some(hit) = self.hits.remove(&hid) {
      trace!(
        "detaching, pausing and launching external command for hit={hid}, pid={}",
        hit.pid
      );
      self
        .pending_detach_reactions
        .insert(hid, DetachReaction::LaunchExternal(cmdline_template));
      self
        .tracer
        .request_process_detach(hit.pid, Some(Signal::SIGSTOP), hid)?;
    }
    Ok(())
  }

  pub fn react_on_process_detach(&mut self, hid: u64, pid: Pid) -> color_eyre::Result<()> {
    debug!(
      "reacting on process {pid}(hid: {hid}) detach, reactions: {:?}",
      self.pending_detach_reactions
    );
    if let Some(reaction) = self.pending_detach_reactions.remove(&hid) {
      trace!("reacting on process {pid} detach: {reaction:?}");
      match reaction {
        DetachReaction::LaunchExternal(cmd) => {
          let cmd = shell_words::split(&cmd.replace("{{PID}}", &pid.to_string()))?;
          tokio::process::Command::new(&cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        }
      }
    }
    Ok(())
  }

  fn seccomp_bpf_warning(&self) -> Paragraph<'static> {
    let space = Span::raw(" ");
    let line1 = Line::default().spans(vec![
      " WARNING ".on_light_red().white().bold(),
      space.clone(),
      "seccomp-bpf optimization is enabled. ".into(),
      "Detached tracees and their children will not be able to use execve{,at} syscall. "
        .light_red(),
      "If the tracee to be detached need to exec other programs, ".into(),
      "please run tracexec with ".cyan().bold(),
      cli_flag("--seccomp-bpf=off"),
      ".".into(),
    ]);
    Paragraph::new(vec![line1]).wrap(Wrap { trim: false })
  }
}

pub struct HitManager;

impl HitManager {}

impl StatefulWidget for HitManager {
  type State = HitManagerState;

  fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    Clear.render(area, buf);
    let block = Block::new()
      .title(" Breakpoint Manager ")
      .borders(Borders::ALL)
      .title_alignment(Alignment::Center);
    let list = tui_widget_list::List::new(state.hits.values().cloned().collect_vec());

    if !state.hits.is_empty() && state.list_state.selected.is_none() {
      state.list_state.select(Some(0));
    }

    if state.tracer.seccomp_bpf() {
      let warning = state.seccomp_bpf_warning();
      let warning_height = warning.line_count(area.width) as u16;
      let [warning_area, list_area] =
        Layout::vertical([Constraint::Length(warning_height), Constraint::Min(0)]).areas(area);
      warning.render(warning_area, buf);
      let inner = block.inner(list_area);
      block.render(list_area, buf);
      list.render(inner, buf, &mut state.list_state);
    } else {
      let inner = block.inner(area);
      block.render(area, buf);
      list.render(inner, buf, &mut state.list_state);
    }
  }
}
