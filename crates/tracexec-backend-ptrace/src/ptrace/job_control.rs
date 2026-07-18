use std::{
  collections::{
    HashMap,
    HashSet,
    VecDeque,
  },
  fs,
  io,
  sync::{
    Arc,
    OnceLock,
    atomic::{
      AtomicBool,
      Ordering,
    },
  },
  thread::Thread,
  time::{
    Duration,
    Instant,
  },
};

use nix::unistd::Pid;
use tracing::{
  debug,
  info,
  trace,
  warn,
};

pub(super) const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_millis(10);
const CPU_STALL_THRESHOLD: f64 = 0.20;
const MEMORY_FULL_STALL_THRESHOLD: f64 = 0.02;
const FALLBACK_CPU_UTILIZATION_THRESHOLD: f64 = 0.98;
const EMERGENCY_AVAILABLE_MEMORY_RATIO: f64 = 0.01;
const FALLBACK_AVAILABLE_MEMORY_RATIO: f64 = 0.05;
const MIN_AVAILABLE_MEMORY_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Default)]
pub(super) struct JobControlWakeupState {
  waiting_jobs: AtomicBool,
  worker: OnceLock<Thread>,
}

impl JobControlWakeupState {
  pub(super) fn register_worker(&self, worker: Thread) {
    assert!(
      self.worker.set(worker).is_ok(),
      "job-control wakeup worker was registered twice"
    );
  }

  pub(super) fn has_waiting_jobs(&self) -> bool {
    self.waiting_jobs.load(Ordering::Acquire)
  }

  fn set_waiting_jobs(&self, waiting: bool) {
    let was_waiting = self.waiting_jobs.swap(waiting, Ordering::AcqRel);
    if waiting
      && !was_waiting
      && let Some(worker) = self.worker.get()
    {
      worker.unpark();
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SystemLoad {
  cpu_stall: Option<f64>,
  cpu_utilization: Option<f64>,
  memory_full_stall: Option<f64>,
  available_memory: u64,
  total_memory: u64,
}

impl SystemLoad {
  fn pressure(self) -> Option<Pressure> {
    match self.cpu_stall {
      Some(stalled) if stalled >= CPU_STALL_THRESHOLD => {
        return Some(Pressure::CpuStall { stalled });
      }
      Some(_) => {}
      None => {
        if let Some(utilization) = self.cpu_utilization
          && utilization >= FALLBACK_CPU_UTILIZATION_THRESHOLD
        {
          return Some(Pressure::CpuUtilizationFallback { utilization });
        }
      }
    }

    if let Some(stalled) = self.memory_full_stall
      && stalled >= MEMORY_FULL_STALL_THRESHOLD
    {
      return Some(Pressure::MemoryStall { stalled });
    }

    if self.total_memory > 0 {
      let ratio = if self.memory_full_stall.is_some() {
        EMERGENCY_AVAILABLE_MEMORY_RATIO
      } else {
        FALLBACK_AVAILABLE_MEMORY_RATIO
      };
      let proportional_reserve = (self.total_memory as f64 * ratio) as u64;
      let absolute_reserve = MIN_AVAILABLE_MEMORY_BYTES.min(self.total_memory / 10);
      let reserve = proportional_reserve.max(absolute_reserve);
      if self.available_memory <= reserve {
        return Some(Pressure::MemoryAvailability {
          reserve,
          available: self.available_memory,
          total: self.total_memory,
        });
      }
    }

    None
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pressure {
  CpuStall {
    stalled: f64,
  },
  CpuUtilizationFallback {
    utilization: f64,
  },
  MemoryStall {
    stalled: f64,
  },
  MemoryAvailability {
    reserve: u64,
    available: u64,
    total: u64,
  },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PsiTotals {
  cpu_some: u64,
  memory_full: u64,
}

#[derive(Debug, Clone, Copy)]
struct PsiSample {
  at: Instant,
  totals: PsiTotals,
}

fn stall_ratio(
  previous: PsiSample,
  current: PsiSample,
  total: u64,
  previous_total: u64,
) -> Option<f64> {
  let elapsed = current
    .at
    .saturating_duration_since(previous.at)
    .as_secs_f64();
  let stalled = total.checked_sub(previous_total)? as f64 / 1_000_000.0;
  (elapsed > 0.0).then(|| (stalled / elapsed).clamp(0.0, 1.0))
}

fn parse_psi_total(psi: &str, category: &str) -> io::Result<u64> {
  let line = psi
    .lines()
    .find(|line| line.split_ascii_whitespace().next() == Some(category))
    .ok_or_else(|| invalid_data(format!("{category} PSI line is missing")))?;
  let total = line
    .split_ascii_whitespace()
    .find_map(|field| field.strip_prefix("total="))
    .ok_or_else(|| invalid_data(format!("total is missing from {category} PSI line")))?;
  total
    .parse()
    .map_err(|error| invalid_data(format!("invalid {category} PSI total `{total}`: {error}")))
}

fn read_psi_totals() -> io::Result<PsiTotals> {
  Ok(PsiTotals {
    cpu_some: parse_psi_total(&fs::read_to_string("/proc/pressure/cpu")?, "some")?,
    memory_full: parse_psi_total(&fs::read_to_string("/proc/pressure/memory")?, "full")?,
  })
}

pub(super) trait LoadProbe {
  fn sample(&mut self) -> io::Result<SystemLoad>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuTimes {
  total: u64,
  idle: u64,
}

#[derive(Debug, Default)]
pub(super) struct ProcLoadProbe {
  previous_cpu_times: Option<CpuTimes>,
  previous_psi: Option<PsiSample>,
  psi_supported: Option<bool>,
}

impl LoadProbe for ProcLoadProbe {
  fn sample(&mut self) -> io::Result<SystemLoad> {
    let current_psi = if self.psi_supported == Some(false) {
      None
    } else {
      match read_psi_totals() {
        Ok(totals) => {
          self.psi_supported = Some(true);
          Some(PsiSample {
            at: Instant::now(),
            totals,
          })
        }
        Err(error) => {
          warn!(%error, "Linux PSI is unavailable; job control is using CPU-utilization and memory-availability fallbacks");
          self.psi_supported = Some(false);
          self.previous_psi = None;
          None
        }
      }
    };
    let (cpu_stall, memory_full_stall) = self
      .previous_psi
      .zip(current_psi)
      .map(|(previous, current)| {
        (
          stall_ratio(
            previous,
            current,
            current.totals.cpu_some,
            previous.totals.cpu_some,
          ),
          stall_ratio(
            previous,
            current,
            current.totals.memory_full,
            previous.totals.memory_full,
          ),
        )
      })
      .unwrap_or_default();
    if let Some(current_psi) = current_psi {
      self.previous_psi = Some(current_psi);
    }

    let cpu_utilization = if cpu_stall.is_none() {
      let current_cpu_times = parse_cpu_times(&fs::read_to_string("/proc/stat")?)?;
      let utilization = self.previous_cpu_times.and_then(|previous| {
        let total = current_cpu_times.total.checked_sub(previous.total)?;
        let idle = current_cpu_times.idle.checked_sub(previous.idle)?;
        (total > 0).then(|| 1.0 - idle.min(total) as f64 / total as f64)
      });
      self.previous_cpu_times = Some(current_cpu_times);
      utilization
    } else {
      None
    };

    let (available_memory, total_memory) =
      parse_memory_info(&fs::read_to_string("/proc/meminfo")?)?;

    Ok(SystemLoad {
      cpu_stall,
      cpu_utilization,
      memory_full_stall,
      available_memory,
      total_memory,
    })
  }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn parse_cpu_times(stat: &str) -> io::Result<CpuTimes> {
  let cpu_line = stat
    .lines()
    .find(|line| line.starts_with("cpu "))
    .ok_or_else(|| invalid_data("aggregate CPU line is missing from /proc/stat"))?;
  let values = cpu_line
    .split_ascii_whitespace()
    .skip(1)
    .take(8)
    .map(|value| {
      value
        .parse::<u64>()
        .map_err(|error| invalid_data(format!("invalid CPU counter `{value}`: {error}")))
    })
    .collect::<io::Result<Vec<_>>>()?;
  if values.len() < 4 {
    return Err(invalid_data("too few aggregate CPU counters in /proc/stat"));
  }

  let total = values
    .iter()
    .try_fold(0_u64, |total, value| total.checked_add(*value))
    .ok_or_else(|| invalid_data("aggregate CPU counters overflowed"))?;
  let idle = values[3]
    .checked_add(values.get(4).copied().unwrap_or_default())
    .ok_or_else(|| invalid_data("aggregate idle CPU counters overflowed"))?;
  Ok(CpuTimes { total, idle })
}

fn parse_memory_info(meminfo: &str) -> io::Result<(u64, u64)> {
  fn value(meminfo: &str, key: &str) -> io::Result<u64> {
    let line = meminfo
      .lines()
      .find(|line| line.starts_with(key))
      .ok_or_else(|| invalid_data(format!("{key} is missing from /proc/meminfo")))?;
    let mut fields = line.split_ascii_whitespace();
    let _key = fields.next();
    let kibibytes = fields
      .next()
      .ok_or_else(|| invalid_data(format!("{key} has no value in /proc/meminfo")))?
      .parse::<u64>()
      .map_err(|error| invalid_data(format!("invalid {key} value: {error}")))?;
    kibibytes
      .checked_mul(1024)
      .ok_or_else(|| invalid_data(format!("{key} value overflowed")))
  }

  Ok((
    value(meminfo, "MemAvailable:")?,
    value(meminfo, "MemTotal:")?,
  ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Admission {
  Run,
  Wait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct JobId(u64);

pub(super) struct AutoJobController<P = ProcLoadProbe> {
  // Each admitted process tree is one job. Descendants inherit its admission so
  // a parent can wait for a child without the child being independently queued.
  jobs: HashMap<JobId, usize>,
  memberships: HashMap<Pid, JobId>,
  next_job_id: u64,
  waiting: HashSet<Pid>,
  wait_queue: VecDeque<Pid>,
  completion_permits: usize,
  probe: P,
  last_sample: Option<SystemLoad>,
  last_sample_at: Option<Instant>,
  sample_interval: Duration,
  sample_generation: u64,
  last_recovery_generation: u64,
  wakeup_state: Arc<JobControlWakeupState>,
  probe_failed: bool,
}

impl AutoJobController {
  pub(super) fn new(wakeup_state: Arc<JobControlWakeupState>) -> Self {
    let mut controller = Self::with_probe(
      ProcLoadProbe::default(),
      RESOURCE_SAMPLE_INTERVAL,
      wakeup_state,
    );
    let _ = controller.refresh_load(true);
    info!(
      cpu_stall_threshold = CPU_STALL_THRESHOLD,
      memory_full_stall_threshold = MEMORY_FULL_STALL_THRESHOLD,
      fallback_cpu_utilization_threshold = FALLBACK_CPU_UTILIZATION_THRESHOLD,
      emergency_available_memory_ratio = EMERGENCY_AVAILABLE_MEMORY_RATIO,
      fallback_available_memory_ratio = FALLBACK_AVAILABLE_MEMORY_RATIO,
      sample_interval_ms = RESOURCE_SAMPLE_INTERVAL.as_millis(),
      "resource-based automatic subprocess job control enabled"
    );
    controller
  }
}

impl<P: LoadProbe> AutoJobController<P> {
  fn with_probe(
    probe: P,
    sample_interval: Duration,
    wakeup_state: Arc<JobControlWakeupState>,
  ) -> Self {
    Self {
      jobs: HashMap::new(),
      memberships: HashMap::new(),
      next_job_id: 1,
      waiting: HashSet::new(),
      wait_queue: VecDeque::new(),
      completion_permits: 0,
      probe,
      last_sample: None,
      last_sample_at: None,
      sample_interval,
      sample_generation: 0,
      last_recovery_generation: 0,
      wakeup_state,
      probe_failed: false,
    }
  }

  fn refresh_load(&mut self, force: bool) -> Option<SystemLoad> {
    if !force
      && self
        .last_sample_at
        .is_some_and(|sampled| sampled.elapsed() < self.sample_interval)
    {
      return self.last_sample;
    }

    self.last_sample_at = Some(Instant::now());
    self.sample_generation = self.sample_generation.wrapping_add(1);
    match self.probe.sample() {
      Ok(load) => {
        if self.probe_failed {
          info!("system resource sampling recovered for subprocess job control");
        }
        self.probe_failed = false;
        self.last_sample = Some(load);
        trace!(
          cpu_stall = load.cpu_stall,
          cpu_utilization = load.cpu_utilization,
          memory_full_stall = load.memory_full_stall,
          available_memory = load.available_memory,
          total_memory = load.total_memory,
          "sampled system resources for subprocess job control"
        );
      }
      Err(error) => {
        if !self.probe_failed {
          warn!(%error, "failed to sample system resources; job control will admit jobs without pressure throttling");
        }
        self.probe_failed = true;
        self.last_sample = None;
      }
    }
    self.last_sample
  }

  fn start_job(&mut self, pid: Pid) -> JobId {
    let job_id = JobId(self.next_job_id);
    self.next_job_id = self
      .next_job_id
      .checked_add(1)
      .expect("job-control identifier overflowed");
    assert!(self.jobs.insert(job_id, 1).is_none());
    assert!(self.memberships.insert(pid, job_id).is_none());
    job_id
  }

  pub(super) fn admit(&mut self, pid: Pid) -> Admission {
    if let Some(job_id) = self.memberships.get(&pid).copied() {
      trace!(
        %pid,
        job_id = job_id.0,
        "job control observed an exec in an admitted process tree"
      );
      return Admission::Run;
    }
    if self.waiting.contains(&pid) {
      return Admission::Wait;
    }

    let pressure = if self.jobs.is_empty() {
      // Always allow one controlled subprocess so external load cannot deadlock
      // the tracee indefinitely.
      None
    } else {
      // Keep the feedback window short enough that `make -j` cannot send a
      // large burst through exec before the controller observes its load.
      self.refresh_load(false).and_then(SystemLoad::pressure)
    };

    if let Some(pressure) = pressure {
      self.waiting.insert(pid);
      self.wait_queue.push_back(pid);
      self.update_waiting_wakeup();
      debug!(
        %pid,
        ?pressure,
        running_jobs = self.jobs.len(),
        waiting_jobs = self.waiting.len(),
        "job control paused subprocess at exec syscall exit"
      );
      Admission::Wait
    } else {
      let job_id = self.start_job(pid);
      trace!(
        %pid,
        job_id = job_id.0,
        running_jobs = self.jobs.len(),
        "job control admitted subprocess"
      );
      Admission::Run
    }
  }

  pub(super) fn process_spawned(&mut self, parent: Pid, child: Pid) {
    let Some(job_id) = self.memberships.get(&parent).copied() else {
      return;
    };
    if let Some(existing_job_id) = self.memberships.get(&child).copied() {
      trace!(
        %parent,
        %child,
        job_id = existing_job_id.0,
        "job control observed an already-accounted process child"
      );
      return;
    }

    self.memberships.insert(child, job_id);
    let members = self
      .jobs
      .get_mut(&job_id)
      .expect("job-control membership referenced a missing job");
    *members = members
      .checked_add(1)
      .expect("job-control process count overflowed");
    trace!(
      %parent,
      %child,
      job_id = job_id.0,
      job_members = *members,
      "job control added child to its parent's process tree"
    );
  }

  pub(super) fn process_execed(&mut self, former_tid: Pid, pid: Pid) {
    if former_tid == pid {
      return;
    }

    let Some(job_id) = self.memberships.remove(&former_tid) else {
      return;
    };
    match self.memberships.get(&pid).copied() {
      Some(existing_job_id) => {
        debug_assert_eq!(existing_job_id, job_id);
        let members = self
          .jobs
          .get_mut(&job_id)
          .expect("job-control membership referenced a missing job");
        *members = members
          .checked_sub(1)
          .expect("job-control process count underflowed after thread exec");
        trace!(
          %former_tid,
          %pid,
          job_id = job_id.0,
          job_members = *members,
          "job control merged a thread into its process leader after exec"
        );
      }
      None => {
        self.memberships.insert(pid, job_id);
        trace!(
          %former_tid,
          %pid,
          job_id = job_id.0,
          "job control transferred membership after a thread exec"
        );
      }
    }
  }

  pub(super) fn process_exited(&mut self, pid: Pid) -> bool {
    if self.waiting.remove(&pid) {
      self.wait_queue.retain(|waiting_pid| *waiting_pid != pid);
      self.update_waiting_wakeup();
      debug!(%pid, "subprocess exited while waiting for job-control admission");
      true
    } else if let Some(job_id) = self.memberships.remove(&pid) {
      let members = self
        .jobs
        .get_mut(&job_id)
        .expect("job-control membership referenced a missing job");
      *members = members
        .checked_sub(1)
        .expect("job-control process count underflowed on exit");
      if *members == 0 {
        self.jobs.remove(&job_id);
        self.completion_permits = self.completion_permits.saturating_add(1);
        debug!(
          %pid,
          job_id = job_id.0,
          running_jobs = self.jobs.len(),
          waiting_jobs = self.waiting.len(),
          "job-control process tree completed"
        );
      } else {
        trace!(
          %pid,
          job_id = job_id.0,
          job_members = *members,
          "job-control process exited while descendants remain"
        );
      }
      false
    } else {
      false
    }
  }

  pub(super) fn jobs_ready_to_run(&mut self) -> Vec<Pid> {
    if self.wait_queue.is_empty() {
      self.completion_permits = 0;
      return Vec::new();
    }

    let pressure = self.refresh_load(false).and_then(SystemLoad::pressure);
    let mut ready = Vec::new();
    while self.completion_permits > 0 {
      let Some(pid) = self.start_next_waiting() else {
        break;
      };
      self.completion_permits -= 1;
      ready.push(pid);
    }
    self.completion_permits = 0;

    // A single pressure-free sample grants one additional job. Resuming the
    // whole FIFO from one sample causes a thundering herd and immediately
    // drives the system back into contention.
    if self.last_recovery_generation != self.sample_generation {
      self.last_recovery_generation = self.sample_generation;
      if (pressure.is_none() || self.jobs.is_empty())
        && let Some(pid) = self.start_next_waiting()
      {
        ready.push(pid);
      }
    }

    for pid in &ready {
      debug!(
        %pid,
        running_jobs = self.jobs.len(),
        waiting_jobs = self.waiting.len(),
        "job control resumed subprocess"
      );
    }
    ready
  }

  fn start_next_waiting(&mut self) -> Option<Pid> {
    while let Some(pid) = self.wait_queue.pop_front() {
      if self.waiting.remove(&pid) {
        self.start_job(pid);
        self.update_waiting_wakeup();
        return Some(pid);
      }
    }
    self.update_waiting_wakeup();
    None
  }

  fn update_waiting_wakeup(&self) {
    self.wakeup_state.set_waiting_jobs(!self.waiting.is_empty());
  }
}

#[cfg(test)]
mod tests {
  use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    thread,
  };

  use tracing_test::traced_test;

  use super::*;

  #[derive(Debug, Clone, Copy)]
  enum ProbeValue {
    Load(SystemLoad),
    Error(io::ErrorKind),
  }

  #[derive(Clone)]
  struct FakeProbe(Rc<RefCell<ProbeValue>>);

  impl LoadProbe for FakeProbe {
    fn sample(&mut self) -> io::Result<SystemLoad> {
      match *self.0.borrow() {
        ProbeValue::Load(load) => Ok(load),
        ProbeValue::Error(kind) => Err(io::Error::from(kind)),
      }
    }
  }

  #[test]
  fn queued_work_unparks_resource_worker() {
    let wakeup_state = Arc::new(JobControlWakeupState::default());
    let (woke_tx, woke_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
      thread::park();
      woke_tx.send(()).unwrap();
    });
    wakeup_state.register_worker(worker.thread().clone());

    wakeup_state.set_waiting_jobs(true);
    let woke = woke_rx.recv_timeout(Duration::from_secs(1));
    // Avoid leaving a failed test's helper blocked forever.
    worker.thread().unpark();
    worker.join().unwrap();

    assert!(woke.is_ok(), "resource worker was not notified");
  }

  fn low_load() -> SystemLoad {
    SystemLoad {
      cpu_stall: Some(0.0),
      cpu_utilization: None,
      memory_full_stall: Some(0.0),
      available_memory: 90,
      total_memory: 100,
    }
  }

  fn cpu_pressure() -> SystemLoad {
    SystemLoad {
      cpu_stall: Some(0.99),
      ..low_load()
    }
  }

  fn controller(load: SystemLoad) -> (AutoJobController<FakeProbe>, Rc<RefCell<ProbeValue>>) {
    let value = Rc::new(RefCell::new(ProbeValue::Load(load)));
    (
      AutoJobController::with_probe(
        FakeProbe(value.clone()),
        Duration::ZERO,
        Arc::new(JobControlWakeupState::default()),
      ),
      value,
    )
  }

  #[test]
  fn parses_proc_resource_inputs() {
    assert_eq!(
      parse_cpu_times("cpu  10 2 3 20 5 1 2 1 7 4\ncpu0 1 1 1 1").unwrap(),
      CpuTimes {
        total: 44,
        idle: 25,
      }
    );
    assert_eq!(
      parse_memory_info("MemTotal: 1000 kB\nMemAvailable: 125 kB\n").unwrap(),
      (125 * 1024, 1000 * 1024)
    );
    assert_eq!(
      parse_psi_total(
        "some avg10=1.50 avg60=0.50 avg300=0.10 total=12345\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=67\n",
        "some"
      )
      .unwrap(),
      12345
    );
    assert_eq!(
      parse_psi_total(
        "some avg10=1.50 avg60=0.50 avg300=0.10 total=12345\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=67\n",
        "full"
      )
      .unwrap(),
      67
    );
  }

  #[test]
  fn proc_probe_samples_host_resources() {
    let load = ProcLoadProbe::default().sample().unwrap();
    assert!(load.total_memory > 0);
    assert!(load.available_memory <= load.total_memory);
  }

  #[test]
  fn calculates_psi_stall_ratio_over_sample_window() {
    let at = Instant::now();
    let previous = PsiSample {
      at,
      totals: PsiTotals {
        cpu_some: 10_000,
        memory_full: 0,
      },
    };
    let current = PsiSample {
      at: at + Duration::from_millis(100),
      totals: PsiTotals {
        cpu_some: 30_000,
        memory_full: 0,
      },
    };
    let ratio = stall_ratio(
      previous,
      current,
      current.totals.cpu_some,
      previous.totals.cpu_some,
    )
    .unwrap();
    assert!((ratio - 0.2).abs() < f64::EPSILON);
  }

  #[test]
  fn does_not_limit_job_count_when_resources_are_available() {
    let (mut controller, _) = controller(low_load());
    for raw_pid in 1..=128 {
      assert_eq!(controller.admit(Pid::from_raw(raw_pid)), Admission::Run);
    }
    assert_eq!(controller.jobs.len(), 128);
    assert!(controller.waiting.is_empty());
  }

  #[test]
  fn load_drop_releases_pressure_waiters() {
    let (mut controller, probe) = controller(cpu_pressure());
    assert_eq!(controller.admit(Pid::from_raw(1)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(2)), Admission::Wait);
    assert!(controller.jobs_ready_to_run().is_empty());

    *probe.borrow_mut() = ProbeValue::Load(low_load());
    assert_eq!(controller.jobs_ready_to_run(), [Pid::from_raw(2)]);
  }

  #[test]
  fn completed_job_is_replaced_during_sustained_pressure() {
    let (mut controller, _) = controller(cpu_pressure());
    assert_eq!(controller.admit(Pid::from_raw(1)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(2)), Admission::Wait);

    controller.process_exited(Pid::from_raw(1));
    assert_eq!(controller.jobs_ready_to_run(), [Pid::from_raw(2)]);
  }

  #[test]
  fn memory_pressure_queues_new_jobs() {
    let low_memory = SystemLoad {
      available_memory: 10,
      total_memory: 100,
      ..low_load()
    };
    let (mut controller, _) = controller(low_memory);
    assert_eq!(controller.admit(Pid::from_raw(1)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(2)), Admission::Wait);
  }

  #[test]
  fn reclaimable_memory_below_old_ten_percent_limit_remains_usable() {
    let gibibyte = 1024_u64.pow(3);
    let available_memory = 2 * gibibyte;
    let total_memory = 50 * gibibyte;
    let load = SystemLoad {
      available_memory,
      total_memory,
      ..low_load()
    };
    assert_eq!(load.pressure(), None);
  }

  #[test]
  fn memory_full_stall_queues_new_jobs() {
    let load = SystemLoad {
      memory_full_stall: Some(0.03),
      ..low_load()
    };
    let (mut controller, _) = controller(load);
    assert_eq!(controller.admit(Pid::from_raw(1)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(2)), Admission::Wait);
  }

  #[test]
  fn cpu_utilization_is_only_used_when_psi_is_unavailable() {
    let load = SystemLoad {
      cpu_stall: None,
      cpu_utilization: Some(0.99),
      memory_full_stall: None,
      ..low_load()
    };
    assert!(matches!(
      load.pressure(),
      Some(Pressure::CpuUtilizationFallback { .. })
    ));
  }

  #[test]
  fn full_cpu_utilization_without_psi_stalls_is_not_pressure() {
    let load = SystemLoad {
      cpu_stall: Some(0.0),
      cpu_utilization: Some(1.0),
      ..low_load()
    };
    assert_eq!(load.pressure(), None);
  }

  #[test]
  fn resource_recovery_wakes_one_extra_job_per_sample() {
    let (mut controller, probe) = controller(cpu_pressure());
    assert_eq!(controller.admit(Pid::from_raw(1)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(2)), Admission::Wait);
    assert_eq!(controller.admit(Pid::from_raw(3)), Admission::Wait);
    assert_eq!(controller.admit(Pid::from_raw(4)), Admission::Wait);
    assert!(controller.wakeup_state.has_waiting_jobs());

    *probe.borrow_mut() = ProbeValue::Load(low_load());
    assert_eq!(controller.jobs_ready_to_run(), [Pid::from_raw(2)]);
    assert_eq!(controller.jobs_ready_to_run(), [Pid::from_raw(3)]);
    assert_eq!(controller.jobs_ready_to_run(), [Pid::from_raw(4)]);
    assert!(!controller.wakeup_state.has_waiting_jobs());
  }

  #[test]
  fn repeated_exec_does_not_create_another_job() {
    let (mut controller, _) = controller(low_load());
    let pid = Pid::from_raw(1);
    assert_eq!(controller.admit(pid), Admission::Run);
    assert_eq!(controller.admit(pid), Admission::Run);
    assert_eq!(controller.jobs.len(), 1);
  }

  #[test]
  fn descendant_exec_inherits_admission_without_deadlocking_parent() {
    let (mut controller, _) = controller(cpu_pressure());
    let gcc = Pid::from_raw(1);
    let assembler = Pid::from_raw(2);
    let unrelated_job = Pid::from_raw(3);

    assert_eq!(controller.admit(gcc), Admission::Run);
    controller.process_spawned(gcc, assembler);
    assert_eq!(controller.admit(assembler), Admission::Run);
    assert_eq!(controller.jobs.len(), 1);
    assert_eq!(controller.admit(unrelated_job), Admission::Wait);

    assert!(!controller.process_exited(gcc));
    assert!(controller.jobs_ready_to_run().is_empty());
    assert!(!controller.process_exited(assembler));
    assert_eq!(controller.jobs_ready_to_run(), [unrelated_job]);
  }

  #[test]
  fn exec_from_non_main_thread_keeps_one_process_tree_member() {
    let (mut controller, _) = controller(low_load());
    let leader = Pid::from_raw(1);
    let thread = Pid::from_raw(2);

    assert_eq!(controller.admit(leader), Admission::Run);
    controller.process_spawned(leader, thread);
    controller.process_execed(thread, leader);

    assert_eq!(controller.jobs.len(), 1);
    assert_eq!(controller.memberships.len(), 1);
    assert!(!controller.process_exited(leader));
    assert!(controller.jobs.is_empty());
  }

  #[test]
  fn queued_process_exit_removes_it_from_fifo() {
    let (mut controller, _) = controller(cpu_pressure());
    controller.admit(Pid::from_raw(1));
    controller.admit(Pid::from_raw(2));
    assert!(controller.wakeup_state.has_waiting_jobs());
    assert!(controller.process_exited(Pid::from_raw(2)));
    assert!(controller.jobs_ready_to_run().is_empty());
    assert!(controller.waiting.is_empty());
    assert!(!controller.wakeup_state.has_waiting_jobs());
  }

  #[test]
  fn probe_failure_fails_open_without_a_fixed_job_limit() {
    let (mut controller, probe) = controller(low_load());
    *probe.borrow_mut() = ProbeValue::Error(io::ErrorKind::PermissionDenied);
    assert_eq!(controller.admit(Pid::from_raw(1)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(2)), Admission::Run);
    assert_eq!(controller.admit(Pid::from_raw(3)), Admission::Run);
  }

  #[traced_test]
  #[test]
  fn logs_pause_and_resume_decisions() {
    let (mut controller, _) = controller(cpu_pressure());
    controller.admit(Pid::from_raw(1));
    controller.admit(Pid::from_raw(2));
    controller.process_exited(Pid::from_raw(1));
    controller.jobs_ready_to_run();

    assert!(logs_contain(
      "job control paused subprocess at exec syscall exit"
    ));
    assert!(logs_contain("job control resumed subprocess"));
  }
}
