use std::{
  collections::HashMap,
  ffi::OsStr,
  fs,
  os::{
    fd::AsFd,
    unix::fs::MetadataExt,
  },
  path::{
    Path,
    PathBuf,
  },
  sync::{
    Arc,
    RwLock,
    atomic::{
      AtomicBool,
      Ordering,
    },
  },
  thread::{
    self,
    JoinHandle,
  },
};

use nix::{
  errno::Errno,
  poll::{
    PollFd,
    PollFlags,
    PollTimeout,
    poll,
  },
  sys::inotify::{
    AddWatchFlags,
    InitFlags,
    Inotify,
    InotifyEvent,
    WatchDescriptor,
  },
};
use tracexec_core::proc::{
  CgroupInfo,
  resolve_cgroup_id,
};
use tracing::{
  debug,
  trace,
  warn,
};

const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Poll timeout for the inotify watcher thread (milliseconds).
const POLL_TIMEOUT_MS: u16 = 500;

/// A look-aside cache that maps cgroupv2 inode IDs to their paths.
///
/// The cache is pre-populated by scanning the cgroup filesystem and kept
/// up-to-date by an inotify watcher running in a dedicated thread.
pub struct CgroupCache {
  inner: Arc<RwLock<HashMap<u64, String>>>,
  shutdown: Arc<AtomicBool>,
  watcher_handle: Option<JoinHandle<()>>,
}

impl Default for CgroupCache {
  fn default() -> Self {
    Self::new()
  }
}

impl CgroupCache {
  /// Create a new cache watching `/sys/fs/cgroup`.
  pub fn new() -> Self {
    Self::with_root(PathBuf::from(CGROUP_ROOT))
  }

  /// Create a new cache watching an arbitrary root directory.
  fn with_root(root: PathBuf) -> Self {
    let inner = Arc::new(RwLock::new(HashMap::new()));
    let shutdown = Arc::new(AtomicBool::new(false));

    // Populate the cache and add inotify watches in a single pass so
    // there is no window where events can be missed.
    let inotify = Inotify::init(InitFlags::IN_NONBLOCK).ok();
    let wd_map = if root.exists() {
      inotify.as_ref().map(|ino| {
        let mut wd = HashMap::new();
        let mut cache = HashMap::new();
        scan_and_watch_recursive(ino, &root, &root, &mut wd, &mut cache);
        *inner.write().unwrap() = cache;
        wd
      })
    } else {
      None
    };

    let handle = inotify.and_then(|inotify| {
      let inner = inner.clone();
      let shutdown = shutdown.clone();
      let wd_map = wd_map.unwrap_or_default();
      thread::Builder::new()
        .name("cgroup-inotify".into())
        .spawn(move || run_inotify_watcher(&root, inotify, wd_map, inner, shutdown))
        .ok()
    });

    Self {
      inner,
      shutdown,
      watcher_handle: handle,
    }
  }

  /// Resolve a cgroup ID to its path.
  ///
  /// Returns immediately from cache on hit.  On miss, falls back to a
  /// filesystem walk of `/sys/fs/cgroup` and caches the result for future
  /// look-ups.
  pub fn resolve(&self, cgroup_id: u64) -> CgroupInfo {
    // Fast path
    if let Some(path) = self.inner.read().unwrap().get(&cgroup_id) {
      trace!("cgroup cache hit: {cgroup_id} -> {path}");
      return CgroupInfo::V2 { path: path.clone() };
    }

    // Slow path
    let info = resolve_cgroup_id(cgroup_id);
    if let CgroupInfo::V2 { ref path } = info
      && let Ok(mut map) = self.inner.write()
    {
      trace!("cgroup cache miss: resolved {cgroup_id} -> {path}, inserting into cache");
      map.insert(cgroup_id, path.clone());
    }
    info
  }
}

impl Drop for CgroupCache {
  fn drop(&mut self) {
    self.shutdown.store(true, Ordering::Relaxed);
    if let Some(h) = self.watcher_handle.take() {
      let _ = h.join();
    }
  }
}

// ---------------------------------------------------------------------------
// Scanning helpers
// ---------------------------------------------------------------------------

/// Walk `root`, collect (inode -> relative-path) for every directory,
/// and add inotify watches simultaneously.
fn scan_and_watch_recursive(
  inotify: &Inotify,
  dir: &Path,
  root: &Path,
  wd_map: &mut WdMap,
  cache: &mut HashMap<u64, String>,
) {
  let relative = dir
    .strip_prefix(root)
    .map(|p| {
      if p == Path::new("") {
        "/".to_string()
      } else {
        format!("/{}", p.display())
      }
    })
    .unwrap_or_else(|_| dir.display().to_string());

  match fs::metadata(dir) {
    Ok(meta) => {
      cache.insert(meta.ino(), relative.clone());
    }
    Err(e) => {
      warn!("Failed to stat cgroup dir {dir:?}: {e}");
    }
  }
  match inotify.add_watch(dir, watch_flags()) {
    Ok(wd) => {
      wd_map.insert(wd, relative);
    }
    Err(e) => {
      warn!("Failed to add inotify watch for {dir:?}: {e}");
    }
  }
  match fs::read_dir(dir) {
    Ok(entries) => {
      for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
          scan_and_watch_recursive(inotify, &path, root, wd_map, cache);
        }
      }
    }
    Err(e) => {
      warn!("Failed to scan and watch entries in cgroup dir {dir:?}: {e}");
    }
  }
}

// ---------------------------------------------------------------------------
// Inotify watcher
// ---------------------------------------------------------------------------

fn watch_flags() -> AddWatchFlags {
  AddWatchFlags::IN_CREATE
    | AddWatchFlags::IN_DELETE
    | AddWatchFlags::IN_MOVED_FROM
    | AddWatchFlags::IN_MOVED_TO
    | AddWatchFlags::IN_ONLYDIR
}

/// Maps each watch descriptor to the *relative* cgroup path it watches
/// (e.g. "/" for the root, "/user.slice" for a child).
type WdMap = HashMap<WatchDescriptor, String>;

/// Derive the relative cgroup path for a child named `name` under `parent_rel`.
fn child_relative_path(parent_rel: &str, name: &OsStr) -> String {
  let name = name.to_string_lossy();
  if parent_rel == "/" {
    format!("/{name}")
  } else {
    format!("{parent_rel}/{name}")
  }
}

fn handle_dir_created(
  inotify: &Inotify,
  root: &Path,
  event: &InotifyEvent,
  wd_map: &mut WdMap,
  cache: &RwLock<HashMap<u64, String>>,
) {
  let name = match event.name.as_ref() {
    Some(n) => n,
    None => return,
  };
  let parent_rel = match wd_map.get(&event.wd) {
    Some(p) => p.clone(),
    None => return,
  };
  let child_rel = child_relative_path(&parent_rel, name);
  let abs_path = if parent_rel == "/" {
    root.join(name)
  } else {
    root.join(&parent_rel[1..]).join(name)
  };

  // Scan and watch in a single pass so that directories created between
  // adding watches and populating the cache are not missed.
  let mut new_cache = HashMap::new();
  scan_and_watch_recursive(inotify, &abs_path, root, wd_map, &mut new_cache);
  if !new_cache.is_empty() {
    cache.write().unwrap().extend(new_cache);
  }
  debug!("cgroup created: {child_rel}");
}

fn handle_dir_deleted(
  event: &InotifyEvent,
  wd_map: &mut WdMap,
  cache: &RwLock<HashMap<u64, String>>,
) {
  let name = match event.name.as_ref() {
    Some(n) => n,
    None => return,
  };
  let parent_rel = match wd_map.get(&event.wd) {
    Some(p) => p.clone(),
    None => return,
  };
  let child_rel = child_relative_path(&parent_rel, name);

  // Remove the deleted path and any descendants from the cache.
  let prefix = format!("{child_rel}/");
  cache
    .write()
    .unwrap()
    .retain(|_, v| *v != child_rel && !v.starts_with(&prefix));

  // The kernel auto-removes the watch for the deleted directory.
  // Clean up our wd_map entry (by matching the path).
  wd_map.retain(|_, v| *v != child_rel && !v.starts_with(&prefix));
  debug!("cgroup removed: {child_rel}");
}

fn run_inotify_watcher(
  root: &Path,
  inotify: Inotify,
  mut wd_map: WdMap,
  cache: Arc<RwLock<HashMap<u64, String>>>,
  shutdown: Arc<AtomicBool>,
) {
  let timeout = PollTimeout::from(POLL_TIMEOUT_MS);

  loop {
    if shutdown.load(Ordering::Relaxed) {
      break;
    }

    let mut fds = [PollFd::new(inotify.as_fd(), PollFlags::POLLIN)];
    match poll(&mut fds, timeout) {
      Ok(0) => continue,
      Ok(_) => {
        let events = match inotify.read_events() {
          Ok(e) => e,
          Err(Errno::EAGAIN) => continue,
          Err(e) => {
            warn!("inotify read error: {e}");
            break;
          }
        };

        for event in &events {
          if !event.mask.contains(AddWatchFlags::IN_ISDIR) {
            continue;
          }
          if event
            .mask
            .intersects(AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO)
          {
            handle_dir_created(&inotify, root, event, &mut wd_map, &cache);
          } else if event
            .mask
            .intersects(AddWatchFlags::IN_DELETE | AddWatchFlags::IN_MOVED_FROM)
          {
            handle_dir_deleted(event, &mut wd_map, &cache);
          }
        }
      }
      Err(Errno::EINTR) => continue,
      Err(e) => {
        warn!("cgroup inotify poll error: {e}");
        break;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    os::unix::fs::MetadataExt,
    time::Duration,
  };

  use tempfile::TempDir;

  use super::*;

  #[test]
  fn scan_and_watch_collects_all_dirs() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("a")).unwrap();
    fs::create_dir(root.join("a/b")).unwrap();
    fs::create_dir(root.join("c")).unwrap();

    let inotify = Inotify::init(InitFlags::IN_NONBLOCK).unwrap();
    let mut wd_map = HashMap::new();
    let mut cache = HashMap::new();
    scan_and_watch_recursive(&inotify, root, root, &mut wd_map, &mut cache);
    assert_eq!(cache.len(), 4); // root + a + a/b + c
    assert!(cache.values().any(|v| v == "/"));
    assert!(cache.values().any(|v| v == "/a"));
    assert!(cache.values().any(|v| v == "/a/b"));
    assert!(cache.values().any(|v| v == "/c"));
    // Every watched directory has a wd entry
    assert_eq!(wd_map.len(), 4);
  }

  #[test]
  fn scan_and_watch_empty_root_has_only_root_entry() {
    let dir = TempDir::new().unwrap();
    let inotify = Inotify::init(InitFlags::IN_NONBLOCK).unwrap();
    let mut wd_map = HashMap::new();
    let mut cache = HashMap::new();
    scan_and_watch_recursive(&inotify, dir.path(), dir.path(), &mut wd_map, &mut cache);
    assert_eq!(cache.len(), 1);
    assert!(cache.values().any(|v| v == "/"));
    assert_eq!(wd_map.len(), 1);
  }

  #[test]
  fn resolve_returns_cached_path() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("test")).unwrap();

    let ino = fs::metadata(root.join("test")).unwrap().ino();
    let cache = CgroupCache::with_root(root.to_path_buf());

    let map = cache.inner.read().unwrap();
    assert_eq!(map.get(&ino).map(String::as_str), Some("/test"));
  }

  #[test]
  fn inotify_detects_new_directory() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    let cache = CgroupCache::with_root(root.clone());

    // Create a new directory after the cache was initialised
    fs::create_dir(root.join("new_cgroup")).unwrap();

    // Give the watcher time to react
    std::thread::sleep(Duration::from_secs(2));

    let ino = fs::metadata(root.join("new_cgroup")).unwrap().ino();
    let map = cache.inner.read().unwrap();
    assert_eq!(map.get(&ino).map(String::as_str), Some("/new_cgroup"));
  }

  #[test]
  fn inotify_detects_deleted_directory() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir(root.join("doomed")).unwrap();

    let cache = CgroupCache::with_root(root.clone());
    let ino = fs::metadata(root.join("doomed")).unwrap().ino();
    assert!(cache.inner.read().unwrap().contains_key(&ino));

    // Remove the directory
    fs::remove_dir(root.join("doomed")).unwrap();
    std::thread::sleep(Duration::from_secs(2));

    assert!(!cache.inner.read().unwrap().contains_key(&ino));
  }

  #[test]
  fn inotify_detects_nested_creation() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir(root.join("parent")).unwrap();

    let cache = CgroupCache::with_root(root.clone());

    // Create a nested directory after initial scan
    fs::create_dir(root.join("parent/child")).unwrap();
    std::thread::sleep(Duration::from_secs(2));

    let ino = fs::metadata(root.join("parent/child")).unwrap().ino();
    let map = cache.inner.read().unwrap();
    assert_eq!(map.get(&ino).map(String::as_str), Some("/parent/child"));
  }

  #[test]
  fn shutdown_stops_watcher_thread() {
    let dir = TempDir::new().unwrap();
    let cache = CgroupCache::with_root(dir.path().to_path_buf());
    assert!(cache.watcher_handle.is_some());
    drop(cache); // Should join the watcher thread without hanging
  }
}
