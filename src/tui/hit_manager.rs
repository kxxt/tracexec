use std::{
  collections::{BTreeMap, HashMap},
  process::Stdio,
  sync::Arc,
};

use color_eyre::{eyre::eyre, Section};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use either::Either;
use itertools::{chain, Itertools};
use nix::{sys::signal::Signal, unistd::Pid};
use ratatui::{
  layout::{Alignment, Constraint, Layout},
  prelude::{Buffer, Rect},
  style::{Modifier, Style, Stylize},
  text::{Line, Span},
  widgets::{Block, Borders, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};
use tracing::{debug, trace};
use tui_prompts::{State, TextPrompt, TextState};
use tui_widget_list::{ListBuilder, ListView};

use crate::{
  action::Action,
  tracer::{state::BreakPointStop, BreakPointHit, Tracer},
};

use super::{
  error_popup::InfoPopupState,
  help::{cli_flag, help_item, help_key},
  theme::THEME,
};

#[derive(Debug, Clone)]
struct BreakPointHitEntry {
  bid: u32,
  pid: Pid,
  stop: BreakPointStop,
  breakpoint_pattern: Option<String>,
}

impl BreakPointHitEntry {
  fn paragraph(&self, selected: bool) -> Paragraph<'static> {
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
        Span::raw("("),
        self.breakpoint_pattern.as_ref().map_or_else(
          || Span::styled("deleted", THEME.hit_entry_no_breakpoint_pattern),
          |pattern| Span::styled(pattern.clone(), THEME.hit_entry_breakpoint_pattern),
        ),
        Span::raw(")"),
        space.clone(),
        Span::styled("at", THEME.hit_entry_plain_text),
        space.clone(),
        Span::styled(
          <&'static str>::from(self.stop),
          THEME.hit_entry_breakpoint_stop,
        ),
      ])
      .style(if selected {
        Style::default().add_modifier(Modifier::REVERSED)
      } else {
        Style::default()
      });
    Paragraph::new(line).wrap(Wrap { trim: true })
  }

  fn hit(&self) -> BreakPointHit {
    BreakPointHit {
      bid: self.bid,
      pid: self.pid,
      stop: self.stop,
    }
  }
}

#[derive(Debug, Clone)]
pub enum DetachReaction {
  LaunchExternal(String),
}

#[derive(PartialEq, Clone, Copy)]
enum EditingTarget {
  DefaultCommand,
  CustomCommand { selection: usize },
}

pub struct HitManagerState {
  tracer: Arc<Tracer>,
  counter: u64,
  hits: BTreeMap<u64, BreakPointHitEntry>,
  pending_detach_reactions: HashMap<u64, DetachReaction>,
  list_state: tui_widget_list::ListState,
  pub visible: bool,
  default_external_command: Option<String>,
  editing: Option<EditingTarget>,
  editor_state: TextState<'static>,
}

impl HitManagerState {
  pub fn new(
    tracer: Arc<Tracer>,
    default_external_command: Option<String>,
  ) -> color_eyre::Result<Self> {
    Ok(Self {
      tracer,
      counter: 0,
      hits: BTreeMap::new(),
      pending_detach_reactions: HashMap::new(),
      list_state: tui_widget_list::ListState::default(),
      visible: false,
      default_external_command,
      editing: None,
      editor_state: TextState::new(),
    })
  }

  pub fn count(&self) -> usize {
    self.hits.len()
  }

  pub fn hide(&mut self) {
    self.visible = false;
    self.editing = None;
  }

  pub fn help(&self) -> impl Iterator<Item = Span> {
    if self.editing.is_none() {
      Either::Left(chain!(
        [
          help_item!("Q", "Back"),
          help_item!("R", "Resume\u{00a0}Process"),
          help_item!("D", "Detach\u{00a0}Process"),
          help_item!("E", "Edit\u{00a0}Default\u{00a0}Command")
        ],
        if self.default_external_command.is_some() {
          Some(help_item!(
            "Enter",
            "Detach,\u{00a0}Stop\u{00a0}and\u{00a0}Run\u{00a0}Default\u{00a0}Command"
          ))
        } else {
          None
        },
        [help_item!(
          "Alt+Enter",
          "Detach,\u{00a0}Stop\u{00a0}and\u{00a0}Run\u{00a0}Command"
        ),]
      ))
    } else {
      Either::Right(chain!([
        help_item!("Enter", "Save"),
        help_item!("Ctrl+U", "Clear"),
        help_item!("Esc/Ctrl+C", "Cancel"),
      ],))
    }
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
    if key.code == KeyCode::F(1) && key.modifiers == KeyModifiers::NONE {
      return Some(Action::SetActivePopup(
        crate::action::ActivePopup::InfoPopup(HitManager::help()),
      ));
    }
    if let Some(editing) = self.editing {
      match key.code {
        KeyCode::Enter => {
          if key.modifiers == KeyModifiers::NONE {
            if self.editor_state.value().trim().is_empty() {
              return Some(Action::show_error_popup(
                "Error".to_string(),
                eyre!("Command cannot be empty or whitespace"),
              ));
            }
            self.editing = None;
            match editing {
              EditingTarget::DefaultCommand => {
                self.default_external_command = Some(self.editor_state.value().to_string())
              }
              EditingTarget::CustomCommand { selection } => {
                self.select_near_by(selection);
                let hid = *self.hits.keys().nth(selection).unwrap();
                if let Err(e) =
                  self.detach_pause_and_launch_external(hid, self.editor_state.value().to_string())
                {
                  return Some(Action::show_error_popup(
                    "Error".to_string(),
                    e.with_note(|| "Failed to detach or launch external command"),
                  ));
                }
                return self.close_when_empty();
              }
            }
            return None;
          }
        }
        KeyCode::Esc => {
          self.editing = None;
          return None;
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
          self.editing = None;
          return None;
        }
        _ => {
          self.editor_state.handle_key_event(key);
          return None;
        }
      }
      return None;
    }
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
        KeyCode::Char('e') => {
          self.editing = Some(EditingTarget::DefaultCommand);
          if let Some(command) = self.default_external_command.clone() {
            self.editor_state = TextState::new().with_value(command);
            self.editor_state.move_end();
          }
        }
        KeyCode::Enter => {
          if let Some(selected) = self.list_state.selected {
            let external_command = self.default_external_command.clone()?;
            self.select_near_by(selected);
            let hid = *self.hits.keys().nth(selected).unwrap();
            // "konsole --hold -e gdb -p {{PID}}".to_owned()
            if let Err(e) = self.detach_pause_and_launch_external(hid, external_command) {
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
    } else if key.code == KeyCode::Enter && key.modifiers == KeyModifiers::ALT {
      if let Some(selected) = self.list_state.selected {
        self.editing = Some(EditingTarget::CustomCommand {
          selection: selected,
        });
      }
    }
    None
  }

  pub fn add_hit(&mut self, hit: BreakPointHit) -> u64 {
    let id = self.counter;
    let BreakPointHit { bid, pid, stop } = hit;
    self.hits.insert(
      id,
      BreakPointHitEntry {
        bid,
        pid,
        stop,
        breakpoint_pattern: self.tracer.get_breakpoint_pattern_string(bid),
      },
    );
    self.counter += 1;
    id
  }

  fn select_near_by(&mut self, old: usize) {
    if old > 0 {
      self.list_state.select(Some(old - 1));
    } else if old + 1 < self.hits.len() {
      self.list_state.select(Some(old));
    } else {
      self.list_state.select(None);
    }
  }

  pub fn detach(&mut self, hid: u64) -> color_eyre::Result<()> {
    if let Some(hit) = self.hits.remove(&hid) {
      self.tracer.request_process_detach(hit.hit(), None, hid)?;
    }
    Ok(())
  }

  pub fn resume(&mut self, hid: u64) -> color_eyre::Result<()> {
    if let Some(hit) = self.hits.remove(&hid) {
      self.tracer.request_process_resume(hit.hit())?;
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
        .request_process_detach(hit.hit(), Some(Signal::SIGSTOP.into()), hid)?;
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

  pub fn cursor(&self) -> Option<(u16, u16)> {
    if self.editing.is_some() {
      Some(self.editor_state.cursor())
    } else {
      None
    }
  }
}

pub struct HitManager;

impl HitManager {}

impl StatefulWidget for HitManager {
  type State = HitManagerState;

  fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
    let help_area = Rect {
      x: buf.area.width.saturating_sub(10),
      y: 0,
      width: 10.min(buf.area.width),
      height: 1,
    };
    Clear.render(help_area, buf);
    Line::default()
      .spans(help_item!("F1", "Help"))
      .render(help_area, buf);
    let editor_area = Rect {
      x: 0,
      y: 1,
      width: buf.area.width,
      height: 1,
    };
    Clear.render(editor_area, buf);
    if let Some(editing) = state.editing {
      let editor = TextPrompt::new(
        match editing {
          EditingTarget::DefaultCommand => "default command",
          EditingTarget::CustomCommand { .. } => "command",
        }
        .into(),
      );
      editor.render(editor_area, buf, &mut state.editor_state);
    } else if let Some(command) = state.default_external_command.as_deref() {
      let line = Line::default().spans(vec![
        Span::raw("🚀 default command: "),
        Span::styled(command, THEME.hit_manager_default_command),
      ]);
      line.render(editor_area, buf);
    } else {
      let line = Line::default().spans(vec![
        Span::styled(
          "default command not set. Press ",
          THEME.hit_manager_no_default_command,
        ),
        help_key("E"),
        Span::styled(" to set", THEME.hit_manager_no_default_command),
      ]);
      line.render(editor_area, buf);
    }

    Clear.render(area, buf);
    let block = Block::new()
      .title(" Hit Manager ")
      .borders(Borders::ALL)
      .title_alignment(Alignment::Center);
    let items = state.hits.values().cloned().collect_vec();
    let builder = ListBuilder::new(move |ctx| {
      let item = &items[ctx.index];
      let paragraph = item.paragraph(ctx.is_selected);
      let line_count = paragraph
        .line_count(ctx.cross_axis_size)
        .try_into()
        .unwrap_or(u16::MAX);
      (paragraph, line_count)
    });
    let list = ListView::new(builder, state.hits.len());

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

impl HitManager {
  fn help() -> InfoPopupState {
    InfoPopupState::info(
      "Help".to_string(),
      vec![
        Line::default().spans(vec![
          "The Hit Manager shows the processes that hit breakpoints and become stopped by tracexec. A process can stop at "
            .into(),
          "syscall-enter(right before exec)".cyan().bold(),
          " or ".into(),
          "syscall-exit(right after exec)".cyan().bold(),
          ". ".into(),
        ]),
        #[cfg(feature = "seccomp-bpf")]
        Line::default().spans(vec![
          "By default, tracexec uses seccomp-bpf to speed up ptrace operations so that there is minimal overhead \
          when running programs inside tracexec. ".into(),
          "However, this comes with a limitation that detached tracees and their children will not be able to use \
          execve{,at} syscall. Usually it is shown as the following error: ".red(),
          "Function not implemented".light_red().bold(),
          ". To workaround this problem, run tracexec with ".into(),
          cli_flag("--seccomp-bpf=off"),
          " flag. ".into(),
        ]),
        Line::default().spans(vec![
          "You can detach, resume or detach and launch external commands for the stopped processes. The ".into(),
          "{{PID}}".cyan().bold(),
          " parameter in the external command will be replaced with the PID of the detached and stopped process. ".into(),
          "For example, you can detach a process and launch a debugger to attach to it. \
          Usually you would want to open a new terminal emulator like: ".into(),
          "konsole --hold -e gdb -p {{PID}}".cyan().bold(),
          ". ".into(),
          "This feature is especially useful when you want to debug a subprocess that is executed from a shell script\
          (which might use pipes as stdio) or another complex software.".into(),
          "It is worth mentioning that even if the process is stopped at syscall-enter stop, by the time a debugger \
          attaches to the process, the process should be already past the syscall-exit stop.".into(),
        ]),
      ],
    )
  }
}
