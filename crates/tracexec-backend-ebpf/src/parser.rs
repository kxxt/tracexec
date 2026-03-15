//! Parsers for data structures read back from eBPF ringbuf

use std::collections::BTreeMap;

use enumflags2::BitFlags;
use nix::libc::AT_FDCWD;
use tracexec_core::{
  event::{
    BpfError,
    FriendlyError,
    OutputMsg,
  },
  proc::{
    Cred,
    CredInspectError,
    FileDescriptorInfoCollection,
    cached_string,
    parse_failiable_envp,
  },
  tracer::InspectError,
};

use crate::{
  bpf::{
    cached_cow,
    interface::BpfEventFlags,
    skel::types::{
      exec_event,
      fd_event,
    },
    utf8_lossy_cow_from_bytes_with_nul,
  },
  event::Path,
};

pub fn process_base_filename(eflags: BitFlags<BpfEventFlags>, event: &exec_event) -> OutputMsg {
  if eflags.contains(BpfEventFlags::FILENAME_READ_ERR) {
    OutputMsg::Err(FriendlyError::Bpf(BpfError::Flags))
  } else {
    cached_cow(utf8_lossy_cow_from_bytes_with_nul(&event.base_filename)).into()
  }
}

/// Reassemble filename from dirfd and base_filename
pub fn process_filename(
  base_filename: OutputMsg,
  event: &exec_event,
  cwd: &OutputMsg,
  fdmap: &FileDescriptorInfoCollection,
) -> OutputMsg {
  let is_execveat = unsafe {
    // SAFETY: the eBPF program ensures that this field is initialized
    event.is_execveat.assume_init()
  };
  if !is_execveat || base_filename.is_ok_and(|s| s.starts_with('/')) {
    base_filename
  } else {
    match event.fd {
      AT_FDCWD => cwd.join(base_filename),
      fd => {
        // Check if it is a valid fd
        if let Some(fdinfo) = fdmap.get(fd) {
          fdinfo.path.clone().join(base_filename)
        } else {
          OutputMsg::PartialOk(cached_string(format!(
            "[err: invalid fd: {fd}]/{base_filename}"
          )))
        }
      }
    }
  }
}

pub fn process_cred(
  eflags: BitFlags<BpfEventFlags>,
  event: &exec_event,
  groups: Result<Vec<u32>, CredInspectError>,
) -> Result<Cred, CredInspectError> {
  if let Ok(groups) = groups
    && !eflags.contains(BpfEventFlags::CRED_READ_ERR)
  {
    Ok(Cred {
      groups,
      uid_real: event.uid,
      uid_effective: event.euid,
      uid_saved_set: event.suid,
      uid_fs: event.fsuid,
      gid_real: event.gid,
      gid_effective: event.egid,
      gid_saved_set: event.sgid,
      gid_fs: event.fsgid,
    })
  } else {
    Err(CredInspectError::Inspect)
  }
}

pub fn process_argv(
  eflags: BitFlags<BpfEventFlags>,
  argv: Vec<OutputMsg>,
) -> Result<Vec<OutputMsg>, InspectError> {
  // Failed to read argv pointer
  if eflags.contains(BpfEventFlags::ARGV_READ_ERR) {
    Err(InspectError::EFAULT)
  } else {
    Ok(argv)
  }
}

pub fn process_envp(
  eflags: BitFlags<BpfEventFlags>,
  env: Vec<OutputMsg>,
  has_dash_env: &mut bool,
) -> Result<BTreeMap<OutputMsg, OutputMsg>, InspectError> {
  // Failed to read envp pointer
  if eflags.contains(BpfEventFlags::ENV_READ_ERR) {
    Err(InspectError::EFAULT)
  } else {
    let (envp, has_dash_env_) = parse_failiable_envp(env);
    *has_dash_env = has_dash_env_;
    Ok(envp)
  }
}

pub fn process_path(
  event: &fd_event,
  fs: &str,
  paths: &hashbrown::HashMap<i32, Path>,
) -> OutputMsg {
  match fs {
    "pipefs" => OutputMsg::Ok(cached_string(format!("pipe:[{}]", event.ino))),
    "sockfs" => OutputMsg::Ok(cached_string(format!("socket:[{}]", event.ino))),
    "anon_inodefs" => OutputMsg::Ok(cached_string(format!(
      "anon_inode:{}",
      paths.get(&event.path_id).unwrap().segments[0].as_ref()
    ))),
    _ => paths.get(&event.path_id).unwrap().to_owned().into(),
  }
}

#[cfg(test)]
mod tests {
  use std::mem::MaybeUninit;

  use enumflags2::BitFlags;
  use hashbrown::HashMap;
  use nix::errno::Errno;
  use tracexec_core::{
    event::{
      BpfError,
      FriendlyError,
      OutputMsg,
    },
    proc::{
      CredInspectError,
      FileDescriptorInfo,
      FileDescriptorInfoCollection,
      cached_string,
    },
  };

  use super::*;
  use crate::event::Path;

  fn flags_with(flag: BpfEventFlags) -> BitFlags<BpfEventFlags> {
    BitFlags::<BpfEventFlags>::from_bits_truncate(flag as u32)
  }

  #[test]
  fn test_process_base_filename_error_flag() {
    let event = exec_event::default();
    let out = process_base_filename(flags_with(BpfEventFlags::FILENAME_READ_ERR), &event);
    assert!(matches!(
      out,
      OutputMsg::Err(FriendlyError::Bpf(BpfError::Flags))
    ));
  }

  #[test]
  fn test_process_base_filename_ok() {
    let mut event = exec_event::default();
    event.base_filename[..4].copy_from_slice(b"bin\0");
    let out = process_base_filename(BitFlags::empty(), &event);
    assert_eq!(out.as_ref(), "bin");
    assert!(matches!(out, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_process_filename_non_execveat_passthrough() {
    let mut event = exec_event::default();
    event.is_execveat = MaybeUninit::new(false);
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/cwd".to_string()));
    let out = process_filename(
      base.clone(),
      &event,
      &cwd,
      &FileDescriptorInfoCollection::default(),
    );
    assert_eq!(out, base);
  }

  #[test]
  fn test_process_filename_absolute_passthrough() {
    let mut event = exec_event::default();
    event.is_execveat = MaybeUninit::new(true);
    let base = OutputMsg::Ok(cached_string("/bin/ls".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/cwd".to_string()));
    let out = process_filename(
      base.clone(),
      &event,
      &cwd,
      &FileDescriptorInfoCollection::default(),
    );
    assert_eq!(out, base);
  }

  #[test]
  fn test_process_filename_execveat_cwd_join() {
    let mut event = exec_event::default();
    event.is_execveat = MaybeUninit::new(true);
    event.fd = AT_FDCWD;
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/home/user".to_string()));
    let out = process_filename(base, &event, &cwd, &FileDescriptorInfoCollection::default());
    assert_eq!(out.as_ref(), "/home/user/rel");
    assert!(matches!(out, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_process_filename_execveat_fd_join() {
    let mut event = exec_event::default();
    event.is_execveat = MaybeUninit::new(true);
    event.fd = 5;
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/cwd".to_string()));
    let mut fdmap = FileDescriptorInfoCollection::default();
    let mut info = FileDescriptorInfo::default();
    info.fd = 5;
    info.path = OutputMsg::Ok(cached_string("/tmp".to_string()));
    fdmap.fdinfo.insert(5, info);
    let out = process_filename(base, &event, &cwd, &fdmap);
    assert_eq!(out.as_ref(), "/tmp/rel");
    assert!(matches!(out, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_process_filename_invalid_fd_partial_ok() {
    let mut event = exec_event::default();
    event.is_execveat = MaybeUninit::new(true);
    event.fd = 9;
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/cwd".to_string()));
    let out = process_filename(base, &event, &cwd, &FileDescriptorInfoCollection::default());
    assert_eq!(out.as_ref(), "[err: invalid fd: 9]/\"rel\"");
    assert!(matches!(out, OutputMsg::PartialOk(_)));
  }

  #[test]
  fn test_process_cred_ok() {
    let mut event = exec_event::default();
    event.uid = 1000;
    event.euid = 1001;
    event.suid = 1002;
    event.fsuid = 1003;
    event.gid = 2000;
    event.egid = 2001;
    event.sgid = 2002;
    event.fsgid = 2003;
    let groups = Ok(vec![10, 11, 12]);
    let cred = process_cred(BitFlags::empty(), &event, groups).unwrap();
    assert_eq!(cred.groups, vec![10, 11, 12]);
    assert_eq!(cred.uid_real, 1000);
    assert_eq!(cred.uid_effective, 1001);
    assert_eq!(cred.uid_saved_set, 1002);
    assert_eq!(cred.uid_fs, 1003);
    assert_eq!(cred.gid_real, 2000);
    assert_eq!(cred.gid_effective, 2001);
    assert_eq!(cred.gid_saved_set, 2002);
    assert_eq!(cred.gid_fs, 2003);
  }

  #[test]
  fn test_process_cred_error_paths() {
    let event = exec_event::default();
    let err = process_cred(
      flags_with(BpfEventFlags::CRED_READ_ERR),
      &event,
      Ok(vec![1, 2]),
    );
    assert_eq!(err, Err(CredInspectError::Inspect));

    let err = process_cred(BitFlags::empty(), &event, Err(CredInspectError::Inspect));
    assert_eq!(err, Err(CredInspectError::Inspect));
  }

  #[test]
  fn test_process_argv_error_flag() {
    let res = process_argv(flags_with(BpfEventFlags::ARGV_READ_ERR), vec![]);
    assert_eq!(res, Err(Errno::EFAULT));
  }

  #[test]
  fn test_process_argv_ok() {
    let argv = vec![OutputMsg::Ok(cached_string("echo".to_string()))];
    let res = process_argv(BitFlags::empty(), argv.clone()).unwrap();
    assert_eq!(res, argv);
  }

  #[test]
  fn test_process_envp_error_flag() {
    let mut has_dash_env = false;
    let res = process_envp(
      flags_with(BpfEventFlags::ENV_READ_ERR),
      vec![],
      &mut has_dash_env,
    );
    assert_eq!(res, Err(Errno::EFAULT));
    assert!(!has_dash_env);
  }

  #[test]
  fn test_process_envp_parsing_and_dash_flag() {
    let mut has_dash_env = false;
    let env = vec![
      OutputMsg::Ok(cached_string("FOO=bar".to_string())),
      OutputMsg::Ok(cached_string("-X=1".to_string())),
    ];
    let res = process_envp(BitFlags::empty(), env, &mut has_dash_env).unwrap();
    assert!(has_dash_env);
    assert_eq!(res.len(), 2);
    assert!(res.keys().any(|k| k.as_ref() == "FOO"));
    assert!(res.keys().any(|k| k.as_ref() == "-X"));
  }

  #[test]
  fn test_process_path_variants() {
    let mut event = fd_event::default();
    event.ino = 123;
    event.path_id = 1;

    let mut paths: HashMap<i32, Path> = HashMap::new();
    paths.insert(
      1,
      Path {
        is_absolute: false,
        segments: vec![OutputMsg::Ok(cached_string("eventpoll".to_string()))],
      },
    );

    let pipe = process_path(&event, "pipefs", &paths);
    assert_eq!(pipe.as_ref(), "pipe:[123]");

    let sock = process_path(&event, "sockfs", &paths);
    assert_eq!(sock.as_ref(), "socket:[123]");

    let anon = process_path(&event, "anon_inodefs", &paths);
    assert_eq!(anon.as_ref(), "anon_inode:eventpoll");

    paths.insert(
      1,
      Path {
        is_absolute: true,
        segments: vec![
          OutputMsg::Ok(cached_string("bin".to_string())),
          OutputMsg::Ok(cached_string("usr".to_string())),
        ],
      },
    );
    let normal = process_path(&event, "ext4", &paths);
    assert_eq!(normal.as_ref(), "/usr/bin");
    assert!(matches!(normal, OutputMsg::Ok(_)));
  }
}
