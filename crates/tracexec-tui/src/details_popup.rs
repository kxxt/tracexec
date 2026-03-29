use std::ops::{
  Deref,
  DerefMut,
};

use arboard::Clipboard;
use crossterm::event::KeyEvent;
use hashbrown::HashMap;
use itertools::{
  Itertools,
  chain,
};
use nix::{
  errno::Errno,
  fcntl::OFlag,
  unistd::{
    Gid,
    Group,
    User,
  },
};
use ratatui::{
  buffer::Buffer,
  layout::{
    Alignment,
    Rect,
    Size,
  },
  style::Styled,
  text::{
    Line,
    Span,
  },
  widgets::{
    Block,
    Borders,
    Clear,
    Paragraph,
    StatefulWidget,
    StatefulWidgetRef,
    Tabs,
    Widget,
    Wrap,
  },
};
use tracexec_core::{
  cli::keys::TuiKeyBindings,
  event::{
    EventId,
    EventStatus,
    ParentEvent,
    TracerEventDetails,
  },
  primitives::local_chan::LocalUnboundedSender,
  proc::CgroupInfo,
};
use tui_scrollview::{
  ScrollView,
  ScrollViewState,
};

use super::{
  error_popup::{
    err_popup_goto_parent_miss,
    err_popup_goto_parent_not_found,
  },
  event_list::{
    Event,
    EventList,
  },
  help::{
    help_desc,
    help_item,
    help_key,
  },
  theme::Theme,
};
use crate::{
  action::{
    Action,
    ActivePopup,
  },
  event::TracerEventDetailsTuiExt,
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
  theme: &'static Theme,
  active_index: usize,
  scroll: ScrollViewState,
  env: Option<Vec<Line<'static>>>,
  fdinfo: Option<Vec<Line<'static>>>,
  available_tabs: Vec<&'static str>,
  tab_index: usize,
  parent_id: Option<EventId>,
}

impl DetailsPopupState {
  pub fn new(event: &Event, list: &EventList) -> Self {
    let theme = list.theme;
    let hide_cloexec_fds = list.modifier_args.hide_cloexec_fds;
    let mut modifier_args = Default::default();
    let rt_modifier = Default::default();
    let mut details = vec![];
    // timestamp
    if let Some(ts) = event.details.timestamp() {
      // Use naive_utc() in tests to avoid timezone-dependent snapshot output
      #[cfg(not(test))]
      let formatted = ts.format("%c").to_string();
      #[cfg(test)]
      let formatted = ts.naive_utc().format("%c").to_string();
      details.push((" Timestamp ", Line::raw(formatted)));
    }
    if let Some(elapsed) = event.elapsed.and_then(|x| x.to_std().ok()) {
      details.push((
        " Duration ",
        Line::raw(humantime::format_duration(elapsed).to_string()),
      ));
    }
    details.push((
      if matches!(event.details.as_ref(), TracerEventDetails::Exec(_)) {
        " Cmdline "
      } else {
        " Details "
      },
      event.details.to_tui_line(
        &list.baseline,
        true,
        &modifier_args,
        rt_modifier,
        None,
        list.theme,
      ),
    ));
    let (env, fdinfo, available_tabs, parent_id) = if let TracerEventDetails::Exec(exec) =
      event.details.as_ref()
    {
      details.extend([
        (" Pid ", Line::from(exec.pid.to_string())),
        (" Exec Syscall ", Line::from(exec.syscall.to_string())),
        (" Exec Pid ", {
          if exec.exec_pid != exec.pid {
            vec![
              exec.exec_pid.to_string().into(),
              " ".into(),
              "(non-main thread)".set_style(theme.tracer_warning),
            ]
            .into()
          } else {
            exec.exec_pid.to_string().into()
          }
        }),
        (" Syscall Result ", {
          if exec.result == 0 {
            "0 (Success)".set_style(theme.exec_result_success).into()
          } else {
            format!("{} ({})", exec.result, Errno::from_raw(-exec.result as i32))
              .set_style(theme.exec_result_failure)
              .into()
          }
        }),
        (
          " Real UID / Effective UID / Saved UID / FS UID ",
          match &exec.cred {
            Ok(cred) => {
              let mut map = HashMap::new();
              let mut spans = Vec::new();
              for (i, uid) in [
                cred.uid_real,
                cred.uid_effective,
                cred.uid_saved_set,
                cred.uid_fs,
              ]
              .into_iter()
              .enumerate()
              {
                if !map.contains_key(&uid)
                  && let Some(user) = User::from_uid(uid.into()).ok().flatten()
                {
                  map.insert(uid, user.name);
                }
                if let Some(name) = map.get(&uid) {
                  spans.push(name.to_string().set_style(theme.uid_gid_name));
                  spans.push(format!("({uid})").set_style(theme.uid_gid_value));
                } else {
                  spans.push(format!("{uid}").set_style(theme.uid_gid_value));
                }
                if i < 3 {
                  spans.push(" / ".into());
                }
              }
              spans.into()
            }
            Err(e) => vec![e.to_string().set_style(theme.inline_tracer_error)].into(),
          },
        ),
        (
          " Real GID / Effective GID / Saved GID / FS GID ",
          match &exec.cred {
            Ok(cred) => {
              let mut map = HashMap::new();
              let mut spans = Vec::new();
              for (i, gid) in [
                cred.gid_real,
                cred.gid_effective,
                cred.gid_saved_set,
                cred.gid_fs,
              ]
              .into_iter()
              .enumerate()
              {
                if !map.contains_key(&gid)
                  && let Some(user) = Group::from_gid(gid.into()).ok().flatten()
                {
                  map.insert(gid, user.name);
                }
                if let Some(name) = map.get(&gid) {
                  spans.push(name.to_string().set_style(theme.uid_gid_name));
                  spans.push(format!("({gid})").set_style(theme.uid_gid_value));
                } else {
                  spans.push(format!("{gid}").set_style(theme.uid_gid_value));
                }
                if i < 3 {
                  spans.push(" / ".into());
                }
              }
              spans.into()
            }
            Err(e) => vec![e.to_string().set_style(theme.inline_tracer_error)].into(),
          },
        ),
        (
          " Supplemental Groups ",
          match &exec.cred {
            Ok(cred) => {
              if !cred.groups.is_empty() {
                let mut spans = Vec::new();
                for &gid in cred.groups.iter() {
                  if let Some(group) = Group::from_gid(Gid::from_raw(gid)).ok().flatten() {
                    spans.push(group.name.set_style(theme.uid_gid_name));
                    spans.push(format!("({gid})").set_style(theme.uid_gid_value));
                  } else {
                    spans.push(format!("{gid}").set_style(theme.uid_gid_value));
                  }
                  spans.push(" ".into())
                }
                spans.into()
              } else {
                vec!["[ empty ]".set_style(theme.empty_field)].into()
              }
            }
            Err(e) => vec![e.to_string().set_style(theme.inline_tracer_error)].into(),
          },
        ),
        (
          " Cgroup ",
          match &exec.cgroup {
            CgroupInfo::V2 { path } => Line::from(path.clone()),
            CgroupInfo::V1Only => {
              vec!["cgroupv1 only (cgroupv2 not available)".set_style(theme.tracer_warning)].into()
            }
            CgroupInfo::NotCollected => {
              vec!["Not collected".set_style(theme.tracer_warning)].into()
            }
            CgroupInfo::Error(e) => vec![e.to_string().set_style(theme.inline_tracer_error)].into(),
          },
        ),
        (" Process Status ", {
          let formatted = event.status.unwrap().to_string();
          match event.status.unwrap() {
            EventStatus::ExecENOENT | EventStatus::ExecFailure => {
              formatted.set_style(theme.status_exec_error).into()
            }
            EventStatus::ProcessRunning => formatted.set_style(theme.status_process_running).into(),
            EventStatus::ProcessTerminated => {
              formatted.set_style(theme.status_process_terminated).into()
            }
            EventStatus::ProcessAborted => formatted.set_style(theme.status_process_aborted).into(),
            EventStatus::ProcessKilled => formatted.set_style(theme.status_process_killed).into(),
            EventStatus::ProcessInterrupted => {
              formatted.set_style(theme.status_process_interrupted).into()
            }
            EventStatus::ProcessSegfault => {
              formatted.set_style(theme.status_process_segfault).into()
            }
            EventStatus::ProcessIllegalInstruction => {
              formatted.set_style(theme.status_process_sigill).into()
            }
            EventStatus::ProcessExitedNormally => formatted
              .set_style(theme.status_process_exited_normally)
              .into(),
            EventStatus::ProcessExitedAbnormally(_) => formatted
              .set_style(theme.status_process_exited_abnormally)
              .into(),
            EventStatus::ProcessSignaled(_) => {
              formatted.set_style(theme.status_process_signaled).into()
            }
            EventStatus::ProcessPaused => formatted.set_style(theme.status_process_paused).into(),
            EventStatus::ProcessDetached => {
              formatted.set_style(theme.status_process_detached).into()
            }
            EventStatus::InternalError => formatted.set_style(theme.status_internal_failure).into(),
          }
        }),
        (" Cwd ", Span::from(exec.cwd.as_ref().to_owned()).into()),
        (" Comm (Before exec) ", exec.comm.to_string().into()),
        (
          " Filename ",
          Span::from(exec.filename.as_ref().to_owned()).into(),
        ),
        (
          " Interpreters ",
          Line::from(
            exec
              .interpreter
              .as_deref()
              .map(|v| TracerEventDetails::interpreters_to_string(v).into())
              .unwrap_or_else(|| "Unknown".set_style(theme.value_unknown)),
          ),
        ),
        (
          " Stdin ",
          if let Some(stdin) = exec.fdinfo.stdin() {
            stdin.path.to_string().into()
          } else {
            "Closed".set_style(theme.fd_closed).into()
          },
        ),
        (
          " Stdout ",
          if let Some(stdout) = exec.fdinfo.stdout() {
            stdout.path.to_string().into()
          } else {
            "Closed".set_style(theme.fd_closed).into()
          },
        ),
        (
          " Stderr ",
          if let Some(stderr) = exec.fdinfo.stderr() {
            stderr.path.to_string().into()
          } else {
            "Closed".set_style(theme.fd_closed).into()
          },
        ),
      ]);

      let parent_id = if let Some(parent) = list.get_parent(event.id) {
        let (label, inner) = match parent {
          ParentEvent::Become(inner) => (" Parent(Becomer) Cmdline ", inner),
          ParentEvent::Spawn(inner) => (" Parent(Spawner) Cmdline ", inner),
        };
        let p = inner.borrow();
        details.push((
          label,
          p.details.to_tui_line(
            &list.baseline,
            true,
            &modifier_args,
            rt_modifier,
            None,
            list.theme,
          ),
        ));
        Some(p.id)
      } else {
        None
      };
      details.extend([
        (" (Experimental) Cmdline with stdio ", {
          modifier_args.stdio_in_cmdline = true;
          event.details.to_tui_line(
            &list.baseline,
            true,
            &modifier_args,
            rt_modifier,
            None,
            list.theme,
          )
        }),
        (" (Experimental) Cmdline with fds ", {
          modifier_args.fd_in_cmdline = true;
          event.details.to_tui_line(
            &list.baseline,
            true,
            &modifier_args,
            rt_modifier,
            None,
            list.theme,
          )
        }),
        (
          " Argv ",
          TracerEventDetails::argv_to_string(&exec.argv).into(),
        ),
      ]);
      let env = match exec.env_diff.as_ref() {
        Ok(env_diff) => {
          let mut env = env_diff
            .added
            .iter()
            .map(|(key, value)| {
              let spans = vec![
                "+".set_style(theme.plus_sign),
                key.to_string().set_style(theme.added_env_key),
                "=".set_style(theme.equal_sign),
                value.to_string().set_style(theme.added_env_val),
              ];
              Line::default().spans(spans)
            })
            .collect_vec();
          env.extend(
            env_diff
              .removed
              .iter()
              .map(|key| {
                let value = list.baseline.env.get(key).unwrap();
                let spans = vec![
                  "-".set_style(theme.minus_sign),
                  key.to_string().set_style(theme.removed_env_key),
                  "=".set_style(theme.equal_sign),
                  value.to_string().set_style(theme.removed_env_val),
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
                let old = list.baseline.env.get(key).unwrap();
                let spans_old = vec![
                  "-".set_style(theme.minus_sign),
                  key.to_string().set_style(theme.removed_env_key),
                  "=".set_style(theme.equal_sign),
                  old.to_string().set_style(theme.removed_env_val),
                ];
                let spans_new = vec![
                  "+".set_style(theme.plus_sign),
                  key.to_string().set_style(theme.added_env_key),
                  "=".set_style(theme.equal_sign),
                  new.to_string().set_style(theme.added_env_val),
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
            list
              .baseline
              .env
              .iter()
              .filter(|(key, _)| !env_diff.is_modified_or_removed(key))
              .map(|(key, value)| {
                let spans = vec![
                  " ".into(),
                  key.to_string().set_style(theme.unchanged_env_key),
                  "=".set_style(theme.equal_sign),
                  value.to_string().set_style(theme.unchanged_env_val),
                ];
                Line::default().spans(spans)
              }),
          );
          env
        }
        Err(e) => {
          vec![Line::from(format!("Failed to read envp: {e}"))]
        }
      };
      let mut fdinfo = vec![];
      for (&fd, info) in exec.fdinfo.fdinfo.iter() {
        if hide_cloexec_fds && info.flags.contains(OFlag::O_CLOEXEC) {
          continue;
        }
        fdinfo.push(
          vec![
            " File Descriptor ".set_style(theme.fd_label),
            format!(" {fd} ").set_style(theme.fd_number_label),
          ]
          .into(),
        );
        // Path
        fdinfo.push(
          vec![
            "Path".set_style(theme.sublabel),
            ": ".into(),
            info.path.to_string().into(),
          ]
          .into(),
        );
        // Flags
        let flags = info.flags.iter().map(|f| {
          let style = match f {
            OFlag::O_CLOEXEC => theme.open_flag_cloexec, // Close on exec
            OFlag::O_RDONLY | OFlag::O_WRONLY | OFlag::O_RDWR => {
              theme.open_flag_access_mode // Access Mode
            }
            OFlag::O_CREAT
            | OFlag::O_DIRECTORY
            | OFlag::O_EXCL
            | OFlag::O_NOCTTY
            | OFlag::O_NOFOLLOW
            | OFlag::O_TMPFILE
            | OFlag::O_TRUNC => theme.open_flag_creation, // File creation flags
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
              theme.open_flag_status // File status flags
            }
            _ => theme.open_flag_other, // Other flags
          };
          let mut flag_display = String::new();
          bitflags::parser::to_writer(&f, &mut flag_display).unwrap();
          flag_display.push(' ');
          flag_display.set_style(style)
        });
        fdinfo.push(
          chain!(["Flags".set_style(theme.sublabel), ": ".into()], flags)
            .collect_vec()
            .into(),
        );
        // Mount Info
        fdinfo.push(
          vec![
            "Mount Info".set_style(theme.sublabel),
            ": ".into(),
            info.mnt_id.to_string().into(),
            " (".set_style(theme.visual_separator),
            info.mnt.clone().into(),
            ")".set_style(theme.visual_separator),
          ]
          .into(),
        );
        // Pos
        fdinfo.push(
          vec![
            "Position".set_style(theme.sublabel),
            ": ".into(),
            info.pos.to_string().into(),
          ]
          .into(),
        );
        // ino
        fdinfo.push(
          vec![
            "Inode Number".set_style(theme.sublabel),
            ": ".into(),
            info.ino.to_string().into(),
          ]
          .into(),
        );
        // extra
        if !info.extra.is_empty() {
          fdinfo.push("Extra Information:".set_style(theme.sublabel).into());
          fdinfo.extend(
            info
              .extra
              .iter()
              .map(|l| vec!["•".set_style(theme.visual_separator), l.clone().into()].into()),
          );
        }
      }

      (
        Some(env),
        Some(fdinfo),
        vec!["Info", "Environment", "FdInfo"],
        parent_id,
      )
    } else {
      (None, None, vec!["Info"], None)
    };
    Self {
      details,
      theme,
      fdinfo,
      active_index: 0,
      scroll: Default::default(),
      env,
      available_tabs,
      tab_index: 0,
      parent_id,
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

  pub fn update_help(&self, keys: &TuiKeyBindings, items: &mut Vec<Span<'_>>) {
    if self.active_tab() == "Info" {
      items.extend(help_item!(
        format!(
          "{}/{}",
          keys.details_prev_field.display(),
          keys.details_next_field.display()
        ),
        "Move\u{00a0}Focus",
        self.theme
      ));
    }
    items.extend(help_item!(
      format!(
        "{}/{}/{}",
        keys.details_prev_tab.display(),
        keys.details_cycle_tab.display(),
        keys.details_next_tab.display()
      ),
      "Switch\u{00a0}Tab",
      self.theme
    ));
    if self.env.is_some() {
      items.extend(help_item!(
        keys.details_view_parent.display(),
        "View\u{00a0}Parent\u{00a0}Details",
        self.theme
      ));
    }
  }

  pub fn handle_key_event(
    &mut self,
    ke: KeyEvent,
    keys: &TuiKeyBindings,
    clipboard: Option<&mut Clipboard>,
    list: &EventList,
    action_tx: &LocalUnboundedSender<Action>,
  ) -> color_eyre::Result<()> {
    if keys.details_scroll_down.matches(ke) {
      self.scroll_down();
    } else if keys.details_scroll_up.matches(ke) {
      self.scroll_up();
    } else if keys.page_down.matches(ke) {
      self.scroll_page_down();
    } else if keys.page_up.matches(ke) {
      self.scroll_page_up();
    } else if keys.scroll_top.matches(ke) {
      self.scroll_to_top();
    } else if keys.scroll_bottom.matches(ke) {
      self.scroll_to_bottom();
    } else if keys.details_next_tab.matches(ke) {
      self.next_tab();
    } else if keys.details_prev_tab.matches(ke) {
      self.prev_tab();
    } else if keys.details_prev_field.matches(ke) {
      if self.active_tab() == "Info" {
        self.prev();
      }
    } else if keys.details_next_field.matches(ke) {
      if self.active_tab() == "Info" {
        self.next();
      }
    } else if keys.close_popup.matches(ke) {
      action_tx.send(Action::CancelCurrentPopup);
    } else if keys.details_copy.matches(ke) {
      if self.active_tab() == "Info"
        && let Some(clipboard) = clipboard
      {
        clipboard.set_text(self.selected())?;
      }
    } else if keys.details_view_parent.matches(ke) {
      if self.env.is_none() {
        // Do not handle non-exec events
      } else if let Some(id) = self.parent_id {
        if let Some(evt) = list.get(id) {
          action_tx.send(Action::SetActivePopup(ActivePopup::ViewDetails(Self::new(
            &evt.borrow(),
            list,
          ))));
        } else {
          action_tx.send(Action::SetActivePopup(err_popup_goto_parent_miss(
            "View Parent Details Error",
            self.theme,
          )));
        }
      } else {
        action_tx.send(Action::SetActivePopup(err_popup_goto_parent_not_found(
          "View Parent Details Result",
          self.theme,
        )));
      }
    } else if keys.details_cycle_tab.matches(ke) {
      self.circle_tab();
    }
    Ok(())
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
    let theme = state.theme;
    let block = Block::new()
      .title(" Details ")
      .borders(Borders::TOP | Borders::BOTTOM)
      .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    block.render(area, buf);

    // Tabs
    let tabs = Tabs::new(state.available_tabs.clone())
      .highlight_style(theme.active_tab)
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
    tabs.render(Rect::new(start, 0, tabs_width, 1), buf);

    // Tab Info
    let paragraph = match state.tab_index {
      0 => self.info_paragraph(state),
      1 => self.env_paragraph(state),
      2 => self.fd_paragraph(state),
      _ => unreachable!(),
    };

    let size = Size {
      width: paragraph
        .line_width()
        .try_into()
        .unwrap_or(u16::MAX)
        .min(area.width - 1),
      height: paragraph
        .line_count(area.width - 1)
        .try_into()
        .unwrap_or(u16::MAX),
    };
    let mut scrollview = ScrollView::new(size);
    scrollview.render_widget(paragraph, Rect::new(0, 0, size.width, size.height));
    scrollview.render(inner, buf, &mut state.scroll);
  }

  type State = DetailsPopupState;
}

impl DetailsPopup {
  fn label<'a>(&self, content: &'a str, active: bool, theme: &Theme) -> Line<'a> {
    if !active {
      content.set_style(theme.label).into()
    } else {
      let mut spans = vec![
        content.set_style(theme.selected_label),
        " ".into(),
        "<- ".set_style(theme.selection_indicator),
      ];
      if self.enable_copy {
        spans.extend([help_key("C", theme), help_desc("Copy", theme)]);
      }
      spans.into()
    }
  }

  fn info_paragraph(&self, state: &DetailsPopupState) -> Paragraph<'_> {
    let theme = state.theme;
    let text = state
      .details
      .iter()
      .enumerate()
      .flat_map(|(idx, (label, line))| {
        [
          self.label(label, idx == state.active_index, theme),
          line.clone(),
        ]
      })
      .collect_vec();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }

  fn env_paragraph(&self, state: &DetailsPopupState) -> Paragraph<'_> {
    let text = state.env.clone().unwrap();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }

  fn fd_paragraph(&self, state: &DetailsPopupState) -> Paragraph<'_> {
    let text = state.fdinfo.clone().unwrap();
    Paragraph::new(text).wrap(Wrap { trim: false })
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeMap,
    sync::Arc,
  };

  use chrono::{
    TimeZone,
    Utc,
  };
  use insta::assert_snapshot;
  use nix::{
    errno::Errno,
    unistd::Pid,
  };
  use tracexec_core::{
    cache::ArcStr,
    cli::args::ModifierArgs,
    event::{
      ExecEvent,
      ExecSyscall,
      OutputMsg,
      TracerEventDetails,
    },
    proc::{
      BaselineInfo,
      CgroupError,
      CgroupInfo,
      Cred,
      FileDescriptorInfoCollection,
    },
  };

  use super::{
    DetailsPopup,
    DetailsPopupState,
  };
  use crate::{
    event_list::EventList,
    test_utils::{
      test_area_full,
      test_render_stateful_widget_area,
    },
    theme::current_theme,
  };

  fn baseline_for_tests() -> Arc<BaselineInfo> {
    Arc::new(BaselineInfo {
      cwd: OutputMsg::Ok("cwd".into()),
      env: BTreeMap::new(),
      fdinfo: FileDescriptorInfoCollection::new_baseline().unwrap(),
    })
  }

  fn exec_event() -> ExecEvent {
    ExecEvent {
      syscall: ExecSyscall::Execve,
      exec_pid: Pid::from_raw(4242),
      pid: Pid::from_raw(4242),
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
      timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap().into(),
      parent: None,
      cgroup: CgroupInfo::V2 {
        path: "/user.slice/user-1000.slice/session-1.scope".to_string(),
      },
    }
  }

  #[test]
  fn snapshot_details_popup_exec_event() {
    let list = EventList::new(
      baseline_for_tests(),
      false,
      ModifierArgs::default(),
      1024,
      false,
      false,
      true,
      current_theme(),
    );
    let event = super::Event {
      details: Arc::new(TracerEventDetails::Exec(Box::new(exec_event()))),
      status: Some(tracexec_core::event::EventStatus::ProcessRunning),
      elapsed: None,
      id: tracexec_core::event::EventId::new(1),
    };
    let mut state = DetailsPopupState::new(&event, &list);
    let area = test_area_full(80, 18);
    let rendered = test_render_stateful_widget_area(DetailsPopup::new(false), area, &mut state);
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_details_popup_execveat_non_main_thread() {
    let list = EventList::new(
      baseline_for_tests(),
      false,
      ModifierArgs::default(),
      1024,
      false,
      false,
      true,
      current_theme(),
    );
    let mut event = exec_event();
    event.syscall = ExecSyscall::Execveat;
    event.exec_pid = Pid::from_raw(5001);
    let event = super::Event {
      details: Arc::new(TracerEventDetails::Exec(Box::new(event))),
      status: Some(tracexec_core::event::EventStatus::ProcessRunning),
      elapsed: None,
      id: tracexec_core::event::EventId::new(1),
    };
    let mut state = DetailsPopupState::new(&event, &list);
    let area = test_area_full(90, 80);
    let rendered = test_render_stateful_widget_area(DetailsPopup::new(false), area, &mut state);
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_details_popup_cgroupv1_warning() {
    let list = EventList::new(
      baseline_for_tests(),
      false,
      ModifierArgs::default(),
      1024,
      false,
      false,
      true,
      current_theme(),
    );
    let mut event = exec_event();
    event.cgroup = CgroupInfo::V1Only;
    let event = super::Event {
      details: Arc::new(TracerEventDetails::Exec(Box::new(event))),
      status: Some(tracexec_core::event::EventStatus::ProcessRunning),
      elapsed: None,
      id: tracexec_core::event::EventId::new(1),
    };
    let mut state = DetailsPopupState::new(&event, &list);
    let area = test_area_full(90, 80);
    let rendered = test_render_stateful_widget_area(DetailsPopup::new(false), area, &mut state);
    assert_snapshot!(rendered);
  }

  #[test]
  fn snapshot_details_popup_cgroup_error() {
    let list = EventList::new(
      baseline_for_tests(),
      false,
      ModifierArgs::default(),
      1024,
      false,
      false,
      true,
      current_theme(),
    );
    let mut event = exec_event();
    event.cgroup = CgroupInfo::Error(CgroupError::ReadProcCgroup {
      kind: std::io::ErrorKind::NotFound,
    });
    let event = super::Event {
      details: Arc::new(TracerEventDetails::Exec(Box::new(event))),
      status: Some(tracexec_core::event::EventStatus::ProcessRunning),
      elapsed: None,
      id: tracexec_core::event::EventId::new(1),
    };
    let mut state = DetailsPopupState::new(&event, &list);
    let area = test_area_full(90, 80);
    let rendered = test_render_stateful_widget_area(DetailsPopup::new(false), area, &mut state);
    assert_snapshot!(rendered);
  }
}
