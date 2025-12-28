//! Abstractions for converting tracer events into perfetto trace packets

use std::{
  cell::RefCell,
  cmp::Reverse,
  rc::Rc,
  sync::{Arc, atomic::AtomicU64},
};

use hashbrown::{Equivalent, HashMap};
use perfetto_trace_proto::{
  TracePacket, TrackDescriptor,
  track_descriptor::{ChildTracksOrdering, SiblingMergeBehavior, StaticOrDynamicName},
};
use priority_queue::PriorityQueue;
use tracing::debug;

use tracexec_core::{
  event::{
    EventId, ParentEvent, ProcessStateUpdate, ProcessStateUpdateEvent, TracerEvent, TracerMessage,
  },
  proc::BaselineInfo,
};

use crate::packet::{SliceEndInfo, TracePacketCreator};
// We try to maintain as few tracks as possible for performance reasons.
// When a process begin, we mark its track as occupied and won't put other newly spawned processes onto it.
// When a parent process first spawns a child process, we create a new track under the parent's track if
// there is not already a free one.
// When the parent process exits, if all of the child tracks are free, we free its track.
// When a child process exits, we also need to bubble up to check if the ancestor tracks can be freed because
// the parent might have already died. (We don't do reparenting.)
// We should prefer to use top tracks when available to make the trace look less messy.

// When releasing a track after the process that occupied it exits, we change its status by:
// 1) If all the children tracks are free, we set it to free and bubble up
// 2) If any of the children is (child-)occupied, we set it to child-occupied.
// The bubble up works this way:
// We check if our parent's children status as the above process states and update its status
// accordingly and continue to bubble up until it is not free.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
  /// Occupied by a process
  Occupied = 0,
  /// At least one child in the descendents is still occupied.
  ChildOccupied = 1,
  /// Available
  Free = 2,
}

#[nutype::nutype(derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize))]
pub struct TrackUuid(u64);

#[nutype::nutype(derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize))]
pub struct SliceUuid(u64);

type TrackPriorityQueue = PriorityQueue<Track, Priority, hashbrown::DefaultHashBuilder>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Priority {
  /// Prefer it to be free first.
  status: Status,
  /// Then prefer smaller IDs. We allocate RootTrackUuid in a monotically increasing way.
  id: Reverse<TrackUuid>,
}

#[derive(Debug)]
pub struct Track {
  uuid: TrackUuid,
  inner: Rc<RefCell<TrackInner>>,
}

#[derive(Debug)]
pub struct TrackInner {
  // TODO: do we really need to store the id in inner?
  uuid: TrackUuid,
  // Parent track information.
  parent: Option<Rc<RefCell<Self>>>,
  // Child tracks
  tracks: Rc<RefCell<TrackPriorityQueue>>,
}

impl std::hash::Hash for Track {
  // We only hash the uuid field, which is enough.
  // Note that hashing other fields will cause logic errors,
  // because we store it in [`PriorityQueue`] and modify it later,
  // which will change its hash if other fields get hashed here.
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.uuid.hash(state);
  }
}

impl PartialEq for Track {
  fn eq(&self, other: &Self) -> bool {
    self.uuid == other.uuid
  }
}

impl Eq for Track {}

impl Equivalent<Track> for TrackUuid {
  fn equivalent(&self, key: &Track) -> bool {
    &key.uuid == self
  }
}

fn allocate_track(
  parent: Option<&Rc<RefCell<TrackInner>>>,
) -> (Rc<RefCell<TrackInner>>, TrackDescriptor) {
  let uuid = allocate_track_id();
  debug!("Create track! {uuid:?}");
  let inner = Rc::new(RefCell::new(TrackInner {
    uuid,
    parent: parent.cloned(),
    tracks: Rc::new(RefCell::new(TrackPriorityQueue::with_default_hasher())),
  }));
  let parent_uuid = parent.map(|p| p.borrow().uuid);
  let child = Track {
    uuid,
    inner: inner.clone(),
  };
  if let Some(parent) = parent {
    parent.borrow_mut().tracks.borrow_mut().push(
      child,
      Priority {
        status: Status::Occupied,
        id: Reverse(uuid),
      },
    );
  }
  (
    inner,
    TrackDescriptor {
      uuid: Some(uuid.into_inner()),
      parent_uuid: parent_uuid.map(|v| v.into_inner()),
      // description: todo!(),
      // process: todo!(),
      // chrome_process: todo!(),
      // thread: todo!(),
      // chrome_thread: todo!(),
      // counter: todo!(),
      disallow_merging_with_system_tracks: Some(true),
      child_ordering: Some(ChildTracksOrdering::Chronological as _),
      // sibling_order_rank: todo!(),
      sibling_merge_behavior: Some(SiblingMergeBehavior::None.into()),
      static_or_dynamic_name: Some(StaticOrDynamicName::Name(uuid.into_inner().to_string())),
      // sibling_merge_key_field: todo!(),
      ..Default::default()
    },
  )
}

impl TrackInner {
  /// Occupy a free track
  ///
  /// Panics if the track is not free
  pub fn occupy(&self) {
    let Some(parent) = &self.parent else {
      // Virtual root track that is always free
      return;
    };
    let our_parent = parent.borrow_mut();
    let mut our_container = our_parent.tracks.borrow_mut();
    let mut selfp = *our_container.get_priority(&self.uuid).unwrap(); // SAFETY: The parent track must contain its child track
    if selfp.status != Status::Free {
      panic!("Attempting to occupy a non-free track ({:?})", selfp.status);
    }
    selfp.status = Status::Occupied;
    our_container.change_priority(&self.uuid, selfp);
  }

  /// Try to free a track
  fn free_inner(&self, bubbleup: bool) {
    let Some(parent) = &self.parent else {
      // Virtual root track that is always free
      return;
    };
    let our_parent = parent.borrow_mut();
    let mut our_container = our_parent.tracks.borrow_mut();
    let mut selfp = *our_container.get_priority(&self.uuid).unwrap(); // SAFETY: The parent track must contain its child track
    if selfp.status == Status::Free {
      return;
    }
    if bubbleup && selfp.status == Status::Occupied {
      // Not freeing a track during bubble-up if itself is still occupied.
      return;
    }
    let child_status_summary = self.tracks.borrow().iter().map(|(_, p)| p.status).min();
    if child_status_summary.is_none() || child_status_summary == Some(Status::Free) {
      // All children are free or there's no children at all
      selfp.status = Status::Free;
      our_container.change_priority(&self.uuid, selfp);
      debug!(
        "Freed {:?}, Try to Free {:?} since all children are free",
        &self.uuid, our_parent.uuid
      );
      drop(our_container);
      // Bubble up
      our_parent.free_inner(true);
    } else {
      // :(
      debug!("Cannot free {:?}", &self.uuid);
      selfp.status = Status::ChildOccupied;
      our_container.change_priority(&self.uuid, selfp);
    }
  }

  /// Try to free this track
  pub fn free(&self) {
    self.free_inner(false);
  }

  pub fn next_available_child_track(&self) -> Option<Rc<RefCell<Self>>> {
    let tracks = self.tracks.borrow();
    let (child, p) = tracks.peek()?;
    if p.status == Status::Free {
      Some(child.inner.clone())
    } else {
      None
    }
  }
}

pub struct TracePacketProducer {
  /// Virtual root track
  tracks: Track,
  /// Stores the track associated with the event. When we need to handle a child exec event,
  /// we lookup the child track from here.
  /// Once the process exits, we could drop its entry.
  track_map: HashMap<EventId, Rc<RefCell<TrackInner>>>,
  /// Stores inflight processes.
  /// - The key is the first "spawn" exec event of that process.
  /// - The value is its uuid and track uuid which will be used when we end its slice
  inflight: HashMap<EventId, TrackUuid>,
  creator: TracePacketCreator,
}

static TRACK_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn allocate_track_id() -> TrackUuid {
  let next = TRACK_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed); // We are single threaded.
  TrackUuid::new(next)
}

impl TracePacketProducer {
  pub fn new(baseline: Arc<BaselineInfo>) -> (Self, TracePacket) {
    let uuid = allocate_track_id();
    let (creator, packet) = TracePacketCreator::new(baseline);
    (
      Self {
        tracks: Track {
          uuid,
          inner: Rc::new(RefCell::new(TrackInner {
            uuid,
            parent: None,
            tracks: Rc::new(RefCell::new(TrackPriorityQueue::with_default_hasher())),
          })),
        },
        track_map: HashMap::new(),
        inflight: HashMap::new(),
        creator,
      },
      packet,
    )
  }

  pub fn ensure_failure_track(&self) -> (TrackUuid, Option<TrackDescriptor>) {
    // Note that we will just reuse the track uuid of the virtual track
    // Since the virtual track is not included into the generated trace.
    let uuid = self.tracks.uuid;
    if self.tracks.inner.borrow().tracks.borrow().contains(&uuid) {
      return (uuid, None);
    }
    self.tracks.inner.borrow_mut().tracks.borrow_mut().push(
      Track {
        uuid,
        inner: Rc::new(RefCell::new(TrackInner {
          uuid,
          parent: None,
          tracks: Rc::new(RefCell::new(TrackPriorityQueue::with_default_hasher())),
        })),
      },
      Priority {
        // Set it to occupied to avoid putting slices on it.
        status: Status::Occupied,
        id: Reverse(uuid),
      },
    );
    (
      uuid,
      Some(TrackDescriptor {
        uuid: Some(uuid.into_inner()),
        parent_uuid: None,
        // description: todo!(),
        // process: todo!(),
        // chrome_process: todo!(),
        // thread: todo!(),
        // chrome_thread: todo!(),
        // counter: todo!(),
        disallow_merging_with_system_tracks: Some(true),
        child_ordering: Some(ChildTracksOrdering::Chronological as _),
        // sibling_order_rank: todo!(),
        sibling_merge_behavior: Some(SiblingMergeBehavior::None.into()),
        static_or_dynamic_name: Some(StaticOrDynamicName::Name("Global Failures".to_string())),
        // sibling_merge_key_field: todo!(),
        ..Default::default()
      }),
    )
  }

  /// Process a message from tracer and optionally produce some [`TracePacket`]s
  pub fn process(&mut self, message: TracerMessage) -> color_eyre::Result<Vec<TracePacket>> {
    // self.tracks.get();
    debug!("Processing message {message:#?}");
    Ok(match message {
      TracerMessage::Event(TracerEvent { details, id }) => match &details {
        tracexec_core::event::TracerEventDetails::Exec(exec_event) => {
          if exec_event.result != 0 {
            // In ptrace mode, a failed exec event must have a parent, except the root one.
            // But even for the root one, we won't reach here because the failure of the root one will cause tracer to terminate
            // In eBPF mode with system-wide tracing, it is possible when an existing process gets exec failures.
            let Some(parent) = exec_event.parent else {
              // TODO: We should find a free track for this failure event and put the
              // possible future exec success event on the same track.
              // But we don't want failure events to occupy the track for too long
              // as there might not be a following success event at all.

              // Currently put failures on a dedicated track for simplicity.
              let (uuid, desc) = self.ensure_failure_track();
              let packet = self.creator.add_exec_failure(&details, uuid)?;
              let mut packets = Vec::new();
              if let Some(desc) = desc {
                packets.push(self.creator.announce_track(exec_event.timestamp, desc));
              }
              packets.push(packet);
              return Ok(packets);
            };
            // Attach exec failure to parent slice.
            let parent: EventId = parent.into();
            // SAFETY: the parent slice hasn't ended yet.
            let track_uuid = *self.inflight.get(&parent).unwrap();
            debug!(
              "Add exec failure of {} to parent {:?}'s track {:?}",
              exec_event.filename, parent, track_uuid
            );
            let packet = self.creator.add_exec_failure(&details, track_uuid)?;
            return Ok(vec![packet]);
          }
          let Some(parent) = exec_event.parent else {
            // Top level event. We do not attempt to re-use tracks for top-level events
            let (track, desc) = allocate_track(None);
            // The track begins with occupied so we don't need to manually set it
            let track_uuid = track.borrow().uuid;
            self.track_map.insert(id, track);
            self.inflight.insert(id, track_uuid);
            // Return a new slice begin trace packet
            return Ok(vec![
              self.creator.announce_track(exec_event.timestamp, desc),
              self.creator.begin_exec_slice(&details, track_uuid)?,
            ]);
          };
          // A child exec event
          let mut packets = vec![];
          match parent {
            ParentEvent::Become(parent_event_id) => {
              // End the old slice and begin a new slice on the same track
              debug!(
                "Parent {parent_event_id:?} becomes {id:?}: {}",
                exec_event.filename
              );

              // Move the track to new id in track_map
              let track = self.track_map.remove(&parent_event_id).unwrap(); // SAFETY: we have not removed it yet.
              let (_, track_uuid) = self.inflight.remove_entry(&parent_event_id).unwrap();
              self.track_map.insert(id, track);
              self.inflight.insert(id, track_uuid);

              // Emit a slice end event
              packets.push(self.creator.end_exec_slice(
                SliceEndInfo::Exec,
                exec_event.timestamp,
                track_uuid,
              )?);

              // Emit a slice begin event
              packets.push(self.creator.begin_exec_slice(&details, track_uuid)?);
            }
            ParentEvent::Spawn(parent_event_id) => {
              // Get a child track and begin a new slice
              let parent_track = &self.track_map[&parent_event_id];
              let child_track = parent_track.borrow().next_available_child_track();
              let (child_track, desc) = if let Some(child_track) = child_track {
                debug!(
                  "Put {} on free track {:?} at {}",
                  exec_event.filename,
                  child_track.borrow().uuid,
                  exec_event.timestamp
                );
                // Mark the track as occupied
                child_track.borrow_mut().occupy();
                (child_track, None)
              } else {
                let (child_track, desc) = allocate_track(Some(parent_track));
                debug!(
                  "Put {} on newly allocated track {:?}",
                  exec_event.filename,
                  child_track.borrow().uuid
                );
                (child_track, Some(desc))
              };
              let track_uuid = child_track.borrow().uuid;

              // Create track map entry for this child
              self.track_map.insert(id, child_track);
              self.inflight.insert(id, track_uuid);

              // Emit a slice begin event for this child
              if let Some(desc) = desc {
                packets.push(self.creator.announce_track(exec_event.timestamp, desc));
              }
              packets.push(self.creator.begin_exec_slice(&details, track_uuid)?);
            }
          }
          packets
        }
        _ => Vec::new(),
      },
      TracerMessage::StateUpdate(ProcessStateUpdateEvent {
        update,
        pid: _,
        ids,
      }) => {
        let mut retrieve_matching_event = || {
          // Iterate through the ids in reverse order to find the matching exec event
          let Some((matching_event, track_uuid)) = ids
            .iter()
            .rev()
            .find_map(|id| self.inflight.remove_entry(id))
          else {
            // If the status update is for a failed exec event, e.g. a parent spawns a child to exec but failed,
            // there won't be a matching event here since we don't store slices for failed spawns.
            return None;
          };
          Some((matching_event, track_uuid))
        };
        match update {
          ProcessStateUpdate::Exit { status, timestamp } => {
            let Some((matching_event, track_uuid)) = retrieve_matching_event() else {
              return Ok(Vec::new());
            };
            // Emit slice end event
            let packet =
              self
                .creator
                .end_exec_slice(SliceEndInfo::Exited(status), timestamp, track_uuid)?;
            // Try to release track
            let track = self.track_map.remove(&matching_event).unwrap();
            debug!(
              "Process for event {ids:?} exited, freeing track {:?} at {}",
              track.borrow().uuid,
              timestamp
            );
            track.borrow_mut().free();
            vec![packet]
          }
          ProcessStateUpdate::Detached { hid: _, timestamp } => {
            let Some((matching_event, track_uuid)) = retrieve_matching_event() else {
              return Ok(Vec::new());
            };
            // Emit slice end event
            let packet =
              self
                .creator
                .end_exec_slice(SliceEndInfo::Detached, timestamp, track_uuid)?;
            // Try to release track
            let track = self.track_map.remove(&matching_event).unwrap();
            track.borrow_mut().free();
            vec![packet]
          }
          ProcessStateUpdate::BreakPointHit(_break_point_hit) => Vec::new(),
          ProcessStateUpdate::Resumed => Vec::new(),
          ProcessStateUpdate::ResumeError { hit: _, error: _ } => Vec::new(), // TODO: gracefully handle it
          ProcessStateUpdate::DetachError { hit: _, error: _ } => Vec::new(), // TODO: gracefully handle it
        }
      }
      TracerMessage::FatalError(_) => unreachable!(), // handled at recorder level
    })
  }
}
