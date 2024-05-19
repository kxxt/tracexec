use std::{
  ops::{ControlFlow, Deref, DerefMut},
  sync::Arc,
};

use arboard::Clipboard;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use itertools::{chain, Itertools};
use nix::{errno::Errno, fcntl::OFlag};
use ratatui::{
  buffer::Buffer,
  layout::{Alignment::Center, Rect, Size},
  style::Styled,
  text::{Line, Span},
  widgets::{
    Block, Borders, Clear, Paragraph, StatefulWidget, StatefulWidgetRef, Tabs, Widget, WidgetRef,
    Wrap,
  },
};
use tui_scrollview::{ScrollView, ScrollViewState};

use crate::{
  event::{EventStatus, TracerEventDetails},
  proc::BaselineInfo,
};

use super::{
  event_list::Event,
  help::{help_desc, help_key},
  theme::THEME,
};

pub struct DetailsPopup {
  enable_copy: bool,
}

impl DetailsPopup {
  pub fn new(enable_copy: bool) -> Self {
    Self { enable_copy }
  }
}

#[derive(Debug, Clone)]
pub struct DetailsPopupState {
  details: Vec<(&'static str, Line<'static>)>,
  active_index: usize,
  scroll: ScrollViewState,
  env: Option<Vec<Line<'static>>>,
  fdinfo: Option<Vec<Line<'static>>>,
  available_tabs: Vec<&'static str>,
  tab_index: usize,
}

impl DetailsPopupState {
  pub fn new(event: &Event, baseline: Arc<BaselineInfo>) -> Self {
    let mut modifier_args = Default::default();
    let rt_modifier = Default::default();
    let mut details = vec![(
      if matches!(event.details.as_ref(), TracerEventDetails::Exec(_)) {
        " Cmdline "
      } else {
        " Details "
      },
      event
        .details
        .to_tui_line(&baseline, true, &modifier_args, rt_modifier, None),
    )];
    let (env, fdinfo, available_tabs) = if let TracerEventDetails::Exec(exec) =
      event.details.as_ref()
    {
      details.extend([
        (" Cmdline with stdio ", {
          modifier_args.stdio_in_cmdline = true;
          event
            .details
            .to_tui_line(&baseline, true, &modifier_args, rt_modifier, None)
        }),
        (" Cmdline with file descriptors ", {
          modifier_args.fd_in_cmdline = true;
          event
            .details
            .to_tui_line(&baseline, true, &modifier_args, rt_modifier, None)
        }),
        (" Pid ", Line::from(exec.pid.to_string())),
        (" Syscall Result ", {
          if exec.result == 0 {
            "0 (Success)".set_style(THEME.exec_result_success).into()
          } else {
            format!("{} ({})", exec.result, Errno::from_raw(-exec.result as i32))
              .set_style(THEME.exec_result_failure)
              .into()
          }
        }),
        (" Process Status ", {
          let formatted = event.status.unwrap().to_string();
          match event.status.unwrap() {
            EventStatus::ExecENOENT | EventStatus::ExecFailure => {
              formatted.set_style(THEME.status_exec_error).into()
            }
            EventStatus::ProcessRunning => formatted.set_style(THEME.status_process_running).into(),
            EventStatus::ProcessTerminated => {
              formatted.set_style(THEME.status_process_terminated).into()
            }
            EventStatus::ProcessAborted => formatted.set_style(THEME.status_process_aborted).into(),
            EventStatus::ProcessKilled => formatted.set_style(THEME.status_process_killed).into(),
            EventStatus::ProcessInterrupted => {
              formatted.set_style(THEME.status_process_interrupted).into()
            }
            EventStatus::ProcessSegfault => {
              formatted.set_style(THEME.status_process_segfault).into()
            }
            EventStatus::ProcessIllegalInstruction => {
              formatted.set_style(THEME.status_process_sigill).into()
            }
            EventStatus::ProcessExitedNormally => formatted
              .set_style(THEME.status_process_exited_normally)
              .into(),
            EventStatus::ProcessExitedAbnormally(_) => formatted
              .set_style(THEME.status_process_exited_abnormally)
              .into(),
            EventStatus::ProcessSignaled(_) => {
              formatted.set_style(THEME.status_process_signaled).into()
            }
          }
        }),
        (
          " Cwd ",
          Span::from(exec.cwd.to_string_lossy().to_string()).into(),
        ),
        (" Comm (Before exec) ", exec.comm.to_string().into()),
        (
          " Filename ",
          Span::from(TracerEventDetails::filename_to_cow(&exec.filename).into_owned()).into(),
        ),
        (
          " Argv ",
          TracerEventDetails::argv_to_string(&exec.argv).into(),
        ),
        (
          " Interpreters ",
          TracerEventDetails::interpreters_to_string(&exec.interpreter).into(),
        ),
        (
          " Stdin ",
          if let Some(stdin) = exec.fdinfo.stdin() {
            stdin.path.to_string().into()
          } else {
            "Closed".set_style(THEME.fd_closed).into()
          },
        ),
        (
          " Stdout ",
          if let Some(stdout) = exec.fdinfo.stdout() {
            stdout.path.to_string().into()
          } else {
            "Closed".set_style(THEME.fd_closed).into()
          },
        ),
        (
          " Stderr ",
          if let Some(stderr) = exec.fdinfo.stderr() {
            stderr.path.to_string().into()
          } else {
            "Closed".set_style(THEME.fd_closed).into()
          },
        ),
      ]);
      let env = match exec.env_diff.as_ref() {
        Ok(env_diff) => {
          let mut env = env_diff
            .added
            .iter()
            .map(|(key, value)| {
              let spans = vec![
                "+".set_style(THEME.plus_sign),
                key.to_string().set_style(THEME.added_env_key),
                "=".set_style(THEME.equal_sign),
                value.to_string().set_style(THEME.added_env_val),
              ];
              Line::default().spans(spans)
            })
            .collect_vec();
          env.extend(
            env_diff
              .removed
              .iter()
              .map(|key| {
                let value = baseline.env.get(key).unwrap();
                let spans = vec![
                  "-".set_style(THEME.minus_sign),
                  key.to_string().set_style(THEME.removed_env_key),
                  "=".set_style(THEME.equal_sign),
                  value.to_string().set_style(THEME.removed_env_val),
                ];
                Line::default().spans(spans)
              })
              .collect_vec(),
          );
          env.extend(
            env_diff
              .modified
              .iter()
              .flat_map(|(key, new)| {
                let old = baseline.env.get(key).unwrap();
                let spans_old = vec![
                  "-".set_style(THEME.minus_sign),
                  key.to_string().set_style(THEME.removed_env_key),
                  "=".set_style(THEME.equal_sign),
                  old.to_string().set_style(THEME.removed_env_val),
                ];
                let spans_new = vec![
                  "+".set_style(THEME.plus_sign),
                  key.to_string().set_style(THEME.added_env_key),
                  "=".set_style(THEME.equal_sign),
                  new.to_string().set_style(THEME.added_env_val),
                ];
                vec![
                  Line::default().spans(spans_old),
                  Line::default().spans(spans_new),
                ]
              })
              .collect_vec(),
          );
          env.extend(
            // Unchanged env
            baseline
              .env
              .iter()
              .filter(|(key, _)| !env_diff.is_modified_or_removed(key))
              .map(|(key, value)| {
                let spans = vec![
                  " ".into(),
                  key.to_string().set_style(THEME.unchanged_env_key),
                  "=".set_style(THEME.equal_sign),
                  value.to_string().set_style(THEME.unchanged_env_val),
                ];
                Line::default().spans(spans)
              }),
          );
          env
        }
        Err(e) => {
          vec![Line::from(format!("Failed to read envp: {}", e))]
        }
      };
      let mut fdinfo = vec![];
      for (&fd, info) in exec.fdinfo.fdinfo.iter() {
        fdinfo.push(
          vec![
            " File Descriptor ".set_style(THEME.fd_label),
            format!(" {} ", fd).set_style(THEME.fd_number_label),
          ]
          .into(),
        );
        // Path
        fdinfo.push(
          vec![
            "Path".set_style(THEME.sublabel),
            ": ".into(),
            info.path.to_string().into(),
          ]
          .into(),
        );
        // Flags
        let flags = info.flags.iter().map(|f| {
          let style = match f {
            OFlag::O_CLOEXEC => THEME.open_flag_cloexec, // Close on exec
            OFlag::O_RDONLY | OFlag::O_WRONLY | OFlag::O_RDWR => {
              THEME.open_flag_access_mode // Access Mode
            }
            OFlag::O_CREAT
            | OFlag::O_DIRECTORY
            | OFlag::O_EXCL
            | OFlag::O_NOCTTY
            | OFlag::O_NOFOLLOW
            | OFlag::O_TMPFILE
            | OFlag::O_TRUNC => THEME.open_flag_creation, // File creation flags
            #[allow(unreachable_patterns)]
            OFlag::O_APPEND
            | OFlag::O_ASYNC
            | OFlag::O_DIRECT
            | OFlag::O_DSYNC
            | OFlag::O_LARGEFILE // will be 0x0 if __USE_LARGEFILE64
            | OFlag::O_NOATIME
            | OFlag::O_NONBLOCK
            | OFlag::O_NDELAY // Same as O_NONBLOCK
            | OFlag::O_PATH
            | OFlag::O_SYNC => {
              THEME.open_flag_status // File status flags
            }
            _ => THEME.open_flag_other, // Other flags
          };
          let mut flag_display = String::new();
          bitflags::parser::to_writer(&f, &mut flag_display).unwrap();
          flag_display.push(' ');
          flag_display.set_style(style)
        });
        fdinfo.push(
          chain!(["Flags".set_style(THEME.sublabel), ": ".into()], flags)
            .collect_vec()
            .into(),
        );
        // Mount Info
        fdinfo.push(
          vec![
            "Mount Info".set_style(THEME.sublabel),
            ": ".into(),
            info.mnt_id.to_string().into(),
            " (".set_style(THEME.visual_separator),
            info.mnt.clone().into(),
            ")".set_style(THEME.visual_separator),
          ]
          .into(),
        );
        // Pos
        fdinfo.push(
          vec![
            "Position".set_style(THEME.sublabel),
            ": ".into(),
            info.pos.to_string().into(),
          ]
          .into(),
        );
        // ino
        fdinfo.push(
          vec![
            "Inode Number".set_style(THEME.sublabel),
            ": ".into(),
            info.ino.to_string().into(),
          ]
          .into(),
        );
        // extra
        if !info.extra.is_empty() {
          fdinfo.push("Extra Information:".set_style(THEME.sublabel).into());
          fdinfo.extend(
            info
              .extra
              .iter()
              .map(|l| vec!["•".set_style(THEME.visual_separator), l.clone().into()].into()),
          );
        }
      }

      (
        Some(env),
        Some(fdinfo),
        vec!["Info", "Environment", "FdInfo"],
      )
    } else {
      (None, None, vec!["Info"])
    };
    Self {
      details,
      fdinfo,
      active_index: 0,
      scroll: Default::default(),
      env,
      available_tabs,
      tab_index: 0,
    }
  }

  pub fn next(&mut self) {
    self.active_index = (self.active_index + 1).min(self.details.len() - 1);
  }

  pub fn prev(&mut self) {
    self.active_index = self.active_index.saturating_sub(1);
  }

  pub fn selected(&self) -> String {
    self.details[self.active_index].1.to_string()
  }

  pub fn next_tab(&mut self) {
    let old = self.tab_index;
    self.tab_index = (self.tab_index + 1).min(self.available_tabs.len() - 1);
    if old != self.tab_index {
      self.scroll.scroll_to_top();
    }
  }

  pub fn prev_tab(&mut self) {
    let old = self.tab_index;
    self.tab_index = self.tab_index.saturating_sub(1);
    if old != self.tab_index {
      self.scroll.scroll_to_top();
    }
  }

  pub fn circle_tab(&mut self) {
    let old = self.tab_index;
    self.tab_index = (self.tab_index + 1) % self.available_tabs.len();
    if old != self.tab_index {
      self.scroll.scroll_to_top();
    }
  }

  pub fn active_tab(&self) -> &'static str {
    self.available_tabs[self.tab_index]
  }

  pub fn handle_key_event(
    &mut self,
    ke: KeyEvent,
    clipboard: Option<&mut Clipboard>,
  ) -> color_eyre::Result<ControlFlow<()>> {
    if ke.modifiers == KeyModifiers::NONE {
      match ke.code {
        KeyCode::Down | KeyCode::Char('j') => {
          if ke.modifiers == KeyModifiers::CONTROL {
            self.scroll_page_down();
          } else if ke.modifiers == KeyModifiers::NONE {
            self.scroll_down()
          }
        }
        KeyCode::Up | KeyCode::Char('k') => {
          if ke.modifiers == KeyModifiers::CONTROL {
            self.scroll_page_up();
          } else if ke.modifiers == KeyModifiers::NONE {
            self.scroll_up()
          }
        }
        KeyCode::PageDown => {
          self.scroll_page_down();
        }
        KeyCode::PageUp => {
          self.scroll_page_up();
        }
        KeyCode::Home => {
          self.scroll_to_top();
        }
        KeyCode::End => {
          self.scroll_to_bottom();
        }
        KeyCode::Right | KeyCode::Char('l') => {
          self.next_tab();
        }
        KeyCode::Left | KeyCode::Char('h') => {
          self.prev_tab();
        }
        KeyCode::Char('w') => {
          if self.active_tab() == "Info" {
            self.prev();
          }
        }
        KeyCode::Char('s') => {
          if self.active_tab() == "Info" {
            self.next();
          }
        }
        KeyCode::Char('q') => {
          return Ok(ControlFlow::Break(()));
        }
        KeyCode::Char('c') => {
          if self.active_tab() == "Info" {
            if let Some(clipboard) = clipboard {
              clipboard.set_text(self.selected())?;
            }
          }
        }
        KeyCode::Tab => {
          self.circle_tab();
        }
        _ => {}
      }
    }
    Ok(ControlFlow::Continue(()))
  }
}

impl Deref for DetailsPopupState {
  type Target = ScrollViewState;

  fn deref(&self) -> &Self::Target {
    &self.scroll
  }
}

impl DerefMut for DetailsPopupState {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.scroll
  }
}

impl StatefulWidgetRef for DetailsPopup {
  fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut DetailsPopupState) {
    Clear.render(area, buf);
    let block = Block::new()
      .title(" Details ")
      .borders(Borders::TOP | Borders::BOTTOM)
      .title_alignment(Center);
    let inner = block.inner(area);
    block.render(area, buf);

    // Tabs
    let tabs = Tabs::new(state.available_tabs.clone())
      .highlight_style(THEME.active_tab)
      .select(state.tab_index);
    // FIXME: Ratatui's tab does not support alignment
    let screen = buf.area;
    let tabs_width = state
      .available_tabs
      .iter()
      .map(|s| s.len() as u16)
      .sum::<u16>()
      + 2 * state.available_tabs.len() as u16 // space
      + state.available_tabs.len().saturating_sub(1) as u16; // vertical bar
    let start = screen.right().saturating_sub(tabs_width);
    tabs.render_ref(Rect::new(start, 0, tabs_width, 1), buf);

    // Tab Info
    let paragraph = match state.tab_index {
      0 => self.info_paragraph(state),
      1 => self.env_paragraph(state),
      2 => self.fd_paragraph(state),
      _ => unreachable!(),
    };

    let size = Size {
      width: area.width - 1,
      height: paragraph
        .line_count(area.width - 1)
        .try_into()
        .unwrap_or(u16::MAX),
    };
    let mut scrollview = ScrollView::new(size);
    scrollview.render_widget(
      paragraph,
      Rect {
        x: 0,
        y: 0,
        width: size.width,
        height: size.height,
      },
    );
    scrollview.render(inner, buf, &mut state.scroll);
  }

  type State = DetailsPopupState;
}

impl DetailsPopup {
  fn label<'a>(&self, content: &'a str, active: bool) -> Line<'a> {
    if !active {
      content.set_style(THEME.label).into()
    } else {
      let mut spans = vec![
        content.set_style(THEME.selected_label),
        " ".into(),
        "<- ".set_style(THEME.selection_indicator),
      ];
      if self.enable_copy {
        spans.extend([help_key("C"), help_desc("Copy")]);
      }
      spans.into()
    }
  }

  fn info_paragraph(&self, state: &DetailsPopupState) -> Paragraph {
    let text = state
      .details
      .iter()
      .enumerate()
      .flat_map(|(idx, (label, line))| [self.label(label, idx == state.active_index), line.clone()])
      .collect_vec();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }

  fn env_paragraph(&self, state: &DetailsPopupState) -> Paragraph {
    let text = state.env.clone().unwrap();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }

  fn fd_paragraph(&self, state: &DetailsPopupState) -> Paragraph {
    let text = state.fdinfo.clone().unwrap();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }
}
