//! Abstractions for creating Perfetto trace packet for tracexec events

use std::marker::PhantomData;

use chrono::{DateTime, Local};
use perfetto_trace_proto::{
  ClockSnapshot, DebugAnnotation, InternedData, TracePacket, TracePacketDefaults, TrackDescriptor,
  TrackEvent,
  clock_snapshot::clock::BuiltinClocks,
  debug_annotation,
  trace_packet::{Data, OptionalTrustedPacketSequenceId, SequenceFlags},
  track_event::{self, NameField},
};

use crate::{
  event::ExecEvent,
  export::perfetto::{intern::DebugAnnotationInternId, producer::TrackUuid},
  tracer::ProcessExit,
};

const TRUSTED_PKT_SEQ_ID: OptionalTrustedPacketSequenceId =
  OptionalTrustedPacketSequenceId::TrustedPacketSequenceId(114514);

pub struct TracePacketCreator {
  _private: PhantomData<()>,
}

impl TracePacketCreator {
  /// Create a creator and the initial packet that needs to be sent first
  pub fn new() -> (Self, TracePacket) {
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
        _private: PhantomData,
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
    &self,
    event: &ExecEvent,
    track_uuid: TrackUuid,
  ) -> color_eyre::Result<TracePacket> {
    let mut packet = Self::boilerplate();
    packet.timestamp = Some(
      event
        .timestamp
        .timestamp_nanos_opt()
        .expect("date out of range") as u64,
    );
    let debug_annotations = vec![
      DebugAnnotationInternId::Argv.with_array(
        event
          .argv
          .as_deref()
          .ok()
          .map(|v| {
            v.iter()
              .map(|a| DebugAnnotation {
                value: Some(debug_annotation::Value::StringValue(a.to_string())),
                ..Default::default()
              })
              .collect()
          })
          .unwrap_or_default(),
      ),
      DebugAnnotationInternId::Filename.with_string(event.filename.to_string()),
      DebugAnnotationInternId::Cwd.with_string(event.cwd.to_string()),
      DebugAnnotationInternId::SyscallRet.with_int(event.result),
    ];
    let track_event = TrackEvent {
      r#type: Some(track_event::Type::SliceBegin as i32),
      track_uuid: Some(track_uuid.into_inner()),

      debug_annotations,
      name_field: Some(NameField::Name(
        event
          .argv
          .as_deref()
          .ok()
          .and_then(|v| v.first())
          .unwrap_or(&event.filename)
          .to_string(),
      )),
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

  pub fn exec_instant(
    &self,
    event: &ExecEvent,
    track_uuid: TrackUuid,
  ) -> color_eyre::Result<TracePacket> {
    todo!()
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
