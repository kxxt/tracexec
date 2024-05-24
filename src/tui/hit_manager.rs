use std::{
  collections::{BTreeMap, HashMap},
  process::Stdio,
  sync::Arc,
};

use caps::CapSet;
use nix::{sys::signal::Signal, unistd::Pid};
use tracing::debug;

use crate::tracer::{state::BreakPointStop, Tracer};

struct BreakPointHitEntry {
  bid: u32,
  pid: Pid,
  stop: BreakPointStop,
}

pub enum DetachReaction {
  LaunchExternal(String),
}

pub struct HitManagerState {
  has_cap_sys_admin: bool,
  tracer: Arc<Tracer>,
  counter: u64,
  hits: BTreeMap<u64, BreakPointHitEntry>,
  pending_detach_reactions: HashMap<u64, DetachReaction>,
}

impl HitManagerState {
  pub fn new(tracer: Arc<Tracer>) -> color_eyre::Result<Self> {
    let ecap = caps::read(None, CapSet::Effective)?;
    debug!("effective caps: {:?}", ecap);
    debug!("permitted caps: {:?}", caps::read(None, CapSet::Permitted)?);
    debug!(
      "inheritable caps: {:?}",
      caps::read(None, CapSet::Inheritable)?
    );
    Ok(Self {
      has_cap_sys_admin: ecap.contains(&caps::Capability::CAP_SYS_ADMIN),
      tracer,
      counter: 0,
      hits: BTreeMap::new(),
      pending_detach_reactions: HashMap::new(),
    })
  }

  pub fn add_hit(&mut self, bid: u32, pid: Pid, stop: BreakPointStop) -> u64 {
    let id = self.counter;
    self.hits.insert(id, BreakPointHitEntry { bid, pid, stop });
    self.counter += 1;
    // FIXME
    self.pending_detach_reactions.insert(
      id,
      DetachReaction::LaunchExternal("konsole --hold -e gdb -p {{PID}}".to_owned()),
    );
    id
  }

  pub fn detach(&mut self, hid: u64, suspend_seccomp_bpf: bool) -> color_eyre::Result<()> {
    if let Some(hit) = self.hits.remove(&hid) {
      #[cfg(feature = "seccomp-bpf")]
      if suspend_seccomp_bpf {
        self.tracer.request_suspend_seccomp_bpf(hit.pid)?;
      }
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
    suspend_seccomp_bpf: bool,
  ) -> color_eyre::Result<()> {
    if let Some(hit) = self.hits.remove(&hid) {
      self
        .pending_detach_reactions
        .insert(hid, DetachReaction::LaunchExternal(cmdline_template));
      #[cfg(feature = "seccomp-bpf")]
      if suspend_seccomp_bpf {
        self.tracer.request_suspend_seccomp_bpf(hit.pid)?;
      }
      self
        .tracer
        .request_process_detach(hit.pid, Some(Signal::SIGSTOP), hid)?;
    }
    Ok(())
  }

  pub fn react_on_process_detach(&mut self, hid: u64, pid: Pid) -> color_eyre::Result<()> {
    if let Some(reaction) = self.pending_detach_reactions.remove(&hid) {
      match reaction {
        DetachReaction::LaunchExternal(cmd) => {
          let cmd = shell_words::split(&cmd.replace("{{PID}}", &pid.to_string()))?;
          // TODO: don't spawn in current tty
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
}

pub struct HitManager {
  has_cap_sys_admin: bool,
}

impl HitManager {
  pub fn new() -> color_eyre::Result<Self> {
    let ecap = caps::read(None, CapSet::Effective)?;
    debug!("effective caps: {:?}", ecap);
    debug!("permitted caps: {:?}", caps::read(None, CapSet::Permitted)?);
    debug!(
      "inheritable caps: {:?}",
      caps::read(None, CapSet::Inheritable)?
    );
    Ok(Self {
      has_cap_sys_admin: ecap.contains(&caps::Capability::CAP_SYS_ADMIN),
    })
  }
}
