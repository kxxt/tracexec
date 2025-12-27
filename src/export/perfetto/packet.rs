//! Abstractions for creating Perfetto trace packet for tracexec events

use std::{num::NonZeroUsize, sync::Arc};

use chrono::{DateTime, Local};
use perfetto_trace_proto::{
  ClockSnapshot, DebugAnnotation, DebugAnnotationName, EventName, InternedData, InternedString,
  TracePacket, TracePacketDefaults, TrackDescriptor, TrackEvent,
  clock_snapshot::clock::BuiltinClocks,
  debug_annotation,
  trace_packet::{Data, OptionalTrustedPacketSequenceId, SequenceFlags},
  track_event::{self, NameField},
};

use crate::{
  action::{CopyTarget, SupportedShell},
  cli::args::ModifierArgs,
  event::{RuntimeModifier, TracerEventDetails},
  export::perfetto::{
    intern::{DebugAnnotationInternId, ValueInterner, da_interned_string},
    producer::TrackUuid,
  },
  proc::BaselineInfo,
  tracer::ProcessExit,
};

const TRUSTED_PKT_SEQ_ID: OptionalTrustedPacketSequenceId =
  OptionalTrustedPacketSequenceId::TrustedPacketSequenceId(114514);

pub struct TracePacketCreator {
  baseline: Arc<BaselineInfo>,
  modifier_args: ModifierArgs,
  da_string_interner: ValueInterner,
  da_name_interner: ValueInterner,
  event_name_interner: ValueInterner,
}

impl TracePacketCreator {
  /// Create a creator and the initial packet that needs to be sent first
  pub fn new(baseline: Arc<BaselineInfo>) -> (Self, TracePacket) {
    let mut packet = Self::boilerplate();
    // sequence id related
    packet.sequence_flags = Some(SequenceFlags::SeqIncrementalStateCleared as u32);
    packet.previous_packet_dropped = Some(true);
    packet.first_packet_on_sequence = Some(true);
    packet.optional_trusted_packet_sequence_id = Some(TRUSTED_PKT_SEQ_ID);
    packet.trace_packet_defaults = Some(TracePacketDefaults {
      timestamp_clock_id: Some(BuiltinClocks::RealtimeCoarse as u32),
      ..Default::default()
    });
    packet.interned_data = Some(InternedData {
      event_categories: vec![],
      event_names: vec![],
      debug_annotation_names: DebugAnnotationInternId::interned_data(),
      debug_annotation_string_values: vec![],
      ..Default::default()
    });
    packet.data = Some(Data::ClockSnapshot(ClockSnapshot {
      clocks: vec![],
      primary_trace_clock: Some(BuiltinClocks::RealtimeCoarse as i32),
    }));
    (
      Self {
        modifier_args: ModifierArgs::default(),
        da_string_interner: ValueInterner::new(NonZeroUsize::new(114_514).unwrap(), 1),
        da_name_interner: ValueInterner::new(
          NonZeroUsize::new(10240).unwrap(),
          DebugAnnotationInternId::End as u64 + 1,
        ),
        event_name_interner: ValueInterner::new(NonZeroUsize::new(1024).unwrap(), 1),
        baseline,
      },
      packet,
    )
  }

  fn boilerplate() -> TracePacket {
    TracePacket {
      optional_trusted_packet_sequence_id: Some(TRUSTED_PKT_SEQ_ID),
      ..Default::default()
    }
  }

  pub fn announce_track(&self, timestamp: DateTime<Local>, track: TrackDescriptor) -> TracePacket {
    let mut packet = Self::boilerplate();
    packet.data = Some(Data::TrackDescriptor(track));
    packet.timestamp = Some(timestamp.timestamp_nanos_opt().expect("date out of range") as u64);
    packet
  }

  pub fn begin_exec_slice(
    &mut self,
    event_details: &TracerEventDetails,
    track_uuid: TrackUuid,
  ) -> color_eyre::Result<TracePacket> {
    let TracerEventDetails::Exec(event) = event_details else {
      panic!("expected exec event");
    };
    assert_eq!(event.result, 0);
    self.process_exec_event(event_details, track_uuid)
  }

  pub fn add_exec_failure(
    &mut self,
    event_details: &TracerEventDetails,
    track_uuid: TrackUuid,
  ) -> color_eyre::Result<TracePacket> {
    let TracerEventDetails::Exec(event) = event_details else {
      panic!("expected exec event");
    };
    assert_ne!(event.result, 0);
    self.process_exec_event(event_details, track_uuid)
  }

  pub fn process_exec_event(
    &mut self,
    // We need to refactor this TracerEventDetails mess.
    // Technically we only need to use ExecEvent but since we only implemented `text_for_copy`
    // on TracerEventDetails we currently must pass a TracerEventDetails here.
    event_details: &TracerEventDetails,
    track_uuid: TrackUuid,
  ) -> color_eyre::Result<TracePacket> {
    let TracerEventDetails::Exec(event) = event_details else {
      panic!("expected exec event");
    };
    let mut packet = Self::boilerplate();
    packet.timestamp = Some(
      event
        .timestamp
        .timestamp_nanos_opt()
        .expect("date out of range") as u64,
    );
    let mut da_interned_strings: Vec<InternedString> = Vec::new();
    let mut da_interned_names: Vec<DebugAnnotationName> = Vec::new();
    let mut interned_eventname: Option<EventName> = None;
    let debug_annotations = vec![
      DebugAnnotationInternId::Argv.with_array(if let Ok(argv) = event.argv.as_deref() {
        let mut result = vec![];
        for arg in argv {
          result.push(da_interned_string(
            self
              .da_string_interner
              .intern_with(arg.as_ref(), &mut da_interned_strings),
          ));
        }
        result
      } else {
        Vec::new()
      }),
      DebugAnnotationInternId::Filename.with_interned_string(
        self
          .da_string_interner
          .intern_with(event.filename.as_ref(), &mut da_interned_strings),
      ),
      DebugAnnotationInternId::Cwd.with_interned_string(
        self
          .da_string_interner
          .intern_with(event.cwd.as_ref(), &mut da_interned_strings),
      ),
      DebugAnnotationInternId::SyscallRet.with_int(event.result),
      DebugAnnotationInternId::Pid.with_uint(event.pid.as_raw() as _),
      DebugAnnotationInternId::Cmdline.with_string(
        event_details
          .text_for_copy(
            &self.baseline,
            CopyTarget::Commandline(SupportedShell::Bash),
            &self.modifier_args,
            RuntimeModifier {
              show_env: true,
              show_cwd: true,
            },
          )
          .to_string(),
      ),
      DebugAnnotationInternId::Env.with_dict({
        if let Ok(env) = event.envp.as_ref() {
          let mut entries = Vec::new();
          for (k, v) in env.iter() {
            entries.push(DebugAnnotation {
              name_field: Some(debug_annotation::NameField::NameIid(
                self
                  .da_name_interner
                  .intern_with(k.as_ref(), &mut da_interned_names),
              )),
              value: Some(debug_annotation::Value::StringValueIid(
                self
                  .da_string_interner
                  .intern_with(v.as_ref(), &mut da_interned_strings),
              )),
              ..Default::default()
            });
          }
          entries
        } else {
          vec![]
        }
      }),
      DebugAnnotationInternId::Fd.with_dict({
        let fds = &event.fdinfo.as_ref().fdinfo;
        let mut entries = Vec::new();
        let mut buffer = itoa::Buffer::new();
        for (k, v) in fds.iter() {
          let fd_num_str = buffer.format(*k);
          entries.push(DebugAnnotation {
            name_field: Some(debug_annotation::NameField::NameIid(
              self
                .da_name_interner
                .intern_with(fd_num_str, &mut da_interned_names),
            )),
            dict_entries: vec![
              DebugAnnotationInternId::Path.with_interned_string(
                self
                  .da_string_interner
                  .intern_with(v.path.as_ref(), &mut da_interned_strings),
              ),
              DebugAnnotationInternId::Flags.with_interned_string({
                let mut flags = String::new();
                bitflags::parser::to_writer(&v.flags, &mut flags).unwrap();
                self
                  .da_string_interner
                  .intern_owned_with(flags, &mut da_interned_strings)
              }),
              DebugAnnotationInternId::Pos.with_uint(v.pos as _),
              DebugAnnotationInternId::MountId.with_int(v.mnt_id as _),
              DebugAnnotationInternId::Mount.with_interned_string(
                self
                  .da_string_interner
                  .intern_with(v.mnt.as_ref(), &mut da_interned_strings),
              ),
              DebugAnnotationInternId::Extra.with_array({
                let mut extras = vec![];
                for extra in v.extra.iter() {
                  extras.push(da_interned_string(
                    self
                      .da_string_interner
                      .intern_with(extra.as_ref(), &mut da_interned_strings),
                  ));
                }
                extras
              }),
              DebugAnnotationInternId::Inode.with_uint(v.ino),
            ],
            ..Default::default()
          });
        }
        entries
      }),
      DebugAnnotationInternId::Interpreter.with_array({
        let mut intps = Vec::new();
        for intp in event.interpreter.as_deref().unwrap_or_default().iter() {
          intps.push(da_interned_string(
            self
              .da_string_interner
              .intern_owned_with(intp.to_string(), &mut da_interned_strings),
          ));
        }
        intps
      }),
      DebugAnnotationInternId::Cred.with_dict({
        if let Ok(cred) = &event.cred {
          vec![
            DebugAnnotationInternId::RealUid.with_uint(cred.uid_real as _),
            DebugAnnotationInternId::EffectiveUid.with_uint(cred.uid_effective as _),
            DebugAnnotationInternId::SavedUid.with_uint(cred.uid_saved_set as _),
            DebugAnnotationInternId::FsUid.with_uint(cred.uid_fs as _),
            DebugAnnotationInternId::RealGid.with_uint(cred.gid_real as _),
            DebugAnnotationInternId::EffectiveGid.with_uint(cred.gid_effective as _),
            DebugAnnotationInternId::SavedGid.with_uint(cred.gid_saved_set as _),
            DebugAnnotationInternId::FsGid.with_uint(cred.gid_fs as _),
            DebugAnnotationInternId::Groups.with_array(
              cred
                .groups
                .iter()
                .copied()
                .map(|g| DebugAnnotation {
                  value: Some(debug_annotation::Value::UintValue(g as _)),
                  ..Default::default()
                })
                .collect(),
            ),
          ]
        } else {
          Vec::new()
        }
      }),
    ];
    let track_event = TrackEvent {
      r#type: Some(if event.result == 0 {
        track_event::Type::SliceBegin
      } else {
        track_event::Type::Instant
      } as i32),
      track_uuid: Some(track_uuid.into_inner()),

      debug_annotations,
      name_field: Some(NameField::NameIid(
        match self.event_name_interner.intern(
          event
            .argv
            .as_deref()
            .ok()
            .and_then(|v| v.first())
            .unwrap_or(&event.filename)
            .as_ref(),
        ) {
          Ok(iid) => iid,
          Err(value) => {
            let iid = value.iid;
            interned_eventname = Some(value.into());
            iid
          }
        },
      )),
      // category_iids: todo!(),
      // log_message: todo!(),
      // categories: todo!(),
      // flow_ids: todo!(),
      // terminating_flow_ids: todo!(),
      ..Default::default()
    };
    packet.data = Some(Data::TrackEvent(track_event));
    if !da_interned_strings.is_empty()
      || !da_interned_names.is_empty()
      || interned_eventname.is_some()
    {
      packet.interned_data = Some(InternedData {
        event_names: interned_eventname.into_iter().collect(),
        debug_annotation_string_values: da_interned_strings,
        debug_annotation_names: da_interned_names,
        ..Default::default()
      });
    }
    Ok(packet)
  }

  pub fn end_exec_slice(
    &self,
    info: SliceEndInfo,
    timestamp: DateTime<Local>,
    track_uuid: TrackUuid,
  ) -> color_eyre::Result<TracePacket> {
    let mut packet = Self::boilerplate();
    packet.timestamp = Some(timestamp.timestamp_nanos_opt().expect("date out of range") as u64);
    let mut debug_annotations = vec![DebugAnnotationInternId::EndReason.with_string(
      info.end_reason().to_string(), // TODO: intern this
    )];
    match info {
      SliceEndInfo::Detached | SliceEndInfo::Error | SliceEndInfo::Exec => {}
      SliceEndInfo::Exited(ProcessExit::Code(code)) => {
        debug_annotations.push(DebugAnnotationInternId::ExitCode.with_int(code as _));
      }
      SliceEndInfo::Exited(ProcessExit::Signal(sig)) => {
        debug_annotations.push(DebugAnnotationInternId::ExitSignal.with_string(sig.to_string()));
      }
    }
    let track_event = TrackEvent {
      r#type: Some(track_event::Type::SliceEnd as i32),
      track_uuid: Some(track_uuid.into_inner()),
      debug_annotations,
      name_field: None,
      // category_iids: todo!(),
      // log_message: todo!(),
      // categories: todo!(),
      // flow_ids: todo!(),
      // terminating_flow_ids: todo!(),
      ..Default::default()
    };
    packet.data = Some(Data::TrackEvent(track_event));
    Ok(packet)
  }
}

pub enum SliceEndInfo {
  Exec,
  Detached,
  Exited(ProcessExit),
  Error,
}

impl SliceEndInfo {
  pub fn end_reason(&self) -> &'static str {
    match self {
      Self::Exec => "exec",
      Self::Detached => "detached",
      Self::Error => "error",
      Self::Exited(ProcessExit::Code(_)) => "exited",
      Self::Exited(ProcessExit::Signal(_)) => "signaled",
    }
  }
}
