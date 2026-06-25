//! Parsers for data structures read back from eBPF ringbuf

use std::{
  collections::BTreeMap,
  mem::size_of,
};

use enumflags2::{
  BitFlag,
  BitFlags,
};
use nix::libc::{
  AT_FDCWD,
  gid_t,
};
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
      path_segment_event,
      tracexec_event_header,
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

fn output_path_string(path: String, partial: bool) -> OutputMsg {
  if partial {
    OutputMsg::PartialOk(cached_string(path))
  } else {
    OutputMsg::Ok(cached_string(path))
  }
}

fn normal_path(event: &fd_event, paths: &hashbrown::HashMap<i32, Path>) -> OutputMsg {
  paths
    .get(&event.path_id)
    .cloned()
    .map(Into::into)
    .unwrap_or_else(|| OutputMsg::Err(BpfError::Dropped.into()))
}

fn first_path_segment<'a>(
  event: &fd_event,
  paths: &'a hashbrown::HashMap<i32, Path>,
) -> Option<&'a OutputMsg> {
  paths
    .get(&event.path_id)
    .and_then(|path| path.segments.first())
}

fn format_with_first_segment(
  event: &fd_event,
  paths: &hashbrown::HashMap<i32, Path>,
  prefix: &str,
  suffix: &str,
) -> OutputMsg {
  match first_path_segment(event, paths) {
    Some(segment) => output_path_string(
      format!("{prefix}{}{suffix}", segment.as_ref()),
      segment.not_ok(),
    ),
    None => OutputMsg::Err(BpfError::Dropped.into()),
  }
}

fn pseudo_name(event: &fd_event) -> Option<std::borrow::Cow<'_, str>> {
  let name = utf8_lossy_cow_from_bytes_with_nul(&event.pseudo_name);
  if name.is_empty() { None } else { Some(name) }
}

pub fn process_path(
  event: &fd_event,
  fs: &str,
  paths: &hashbrown::HashMap<i32, Path>,
) -> OutputMsg {
  // If an event does not use dynamic dname, treat it like a normal one
  if event.uses_d_dname == 0 {
    return normal_path(event, paths);
  }

  match fs {
    "pipefs" => OutputMsg::Ok(cached_string(format!("pipe:[{}]", event.ino))),
    "sockfs" => OutputMsg::Ok(cached_string(format!("socket:[{}]", event.ino))),
    "anon_inodefs" => format_with_first_segment(event, paths, "anon_inode:", ""),
    "pidfs" => OutputMsg::Ok(cached_string("anon_inode:[pidfd]".to_string())),
    "nsfs" => match pseudo_name(event) {
      Some(name) => OutputMsg::Ok(cached_string(format!("{name}:[{}]", event.ino))),
      None => OutputMsg::Err(BpfError::Dropped.into()),
    },
    "dmabuf" => match first_path_segment(event, paths) {
      Some(segment) => {
        let name = pseudo_name(event);
        output_path_string(
          format!("/{}:{}", segment.as_ref(), name.as_deref().unwrap_or("")),
          segment.not_ok() || name.is_none(),
        )
      }
      None => OutputMsg::Err(BpfError::Dropped.into()),
    },
    _ => format_with_first_segment(event, paths, "/", " (deleted)"),
  }
}

pub fn parse_string_event(header: &tracexec_event_header, data: &[u8]) -> OutputMsg {
  let header_len = size_of::<tracexec_event_header>();
  let flags = BpfEventFlags::from_bits_truncate(header.flags);
  if flags.is_empty() {
    cached_cow(utf8_lossy_cow_from_bytes_with_nul(&data[header_len..])).into()
  } else {
    OutputMsg::Err(FriendlyError::Bpf(BpfError::Flags))
  }
}

pub fn parse_path_segment(data: &[u8]) -> OutputMsg {
  assert_eq!(data.len(), size_of::<path_segment_event>());
  let event: &path_segment_event = unsafe { &*(data.as_ptr() as *const _) };
  let flags = BpfEventFlags::from_bits_truncate(event.header.flags);
  if flags.is_empty() {
    OutputMsg::Ok(cached_cow(utf8_lossy_cow_from_bytes_with_nul(
      &event.segment,
    )))
  } else {
    OutputMsg::Err(FriendlyError::Bpf(BpfError::Flags))
  }
}

pub fn parse_groups_event(data: &[u8]) -> Vec<gid_t> {
  let groups_len = data.len() - size_of::<tracexec_event_header>();
  assert!(groups_len.is_multiple_of(size_of::<gid_t>()));
  let groups: &[gid_t] = bytemuck::cast_slice(&data[size_of::<tracexec_event_header>()..]);
  groups.to_vec()
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
    let event = exec_event {
      is_execveat: MaybeUninit::new(false),
      ..Default::default()
    };
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
    let event = exec_event {
      is_execveat: MaybeUninit::new(true),
      ..Default::default()
    };
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
    let event = exec_event {
      is_execveat: MaybeUninit::new(true),
      fd: AT_FDCWD,
      ..Default::default()
    };
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/home/user".to_string()));
    let out = process_filename(base, &event, &cwd, &FileDescriptorInfoCollection::default());
    assert_eq!(out.as_ref(), "/home/user/rel");
    assert!(matches!(out, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_process_filename_execveat_fd_join() {
    let event = exec_event {
      is_execveat: MaybeUninit::new(true),
      fd: 5,
      ..Default::default()
    };
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/cwd".to_string()));
    let mut fdmap = FileDescriptorInfoCollection::default();
    let info = FileDescriptorInfo {
      fd: 5,
      path: OutputMsg::Ok(cached_string("/tmp".to_string())),
      ..Default::default()
    };
    fdmap.fdinfo.insert(5, info);
    let out = process_filename(base, &event, &cwd, &fdmap);
    assert_eq!(out.as_ref(), "/tmp/rel");
    assert!(matches!(out, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_process_filename_invalid_fd_partial_ok() {
    let event = exec_event {
      is_execveat: MaybeUninit::new(true),
      fd: 9,
      ..Default::default()
    };
    let base = OutputMsg::Ok(cached_string("rel".to_string()));
    let cwd = OutputMsg::Ok(cached_string("/cwd".to_string()));
    let out = process_filename(base, &event, &cwd, &FileDescriptorInfoCollection::default());
    assert_eq!(out.as_ref(), "[err: invalid fd: 9]/\"rel\"");
    assert!(matches!(out, OutputMsg::PartialOk(_)));
  }

  #[test]
  fn test_process_cred_ok() {
    let event = exec_event {
      uid: 1000,
      euid: 1001,
      suid: 1002,
      fsuid: 1003,
      gid: 2000,
      egid: 2001,
      sgid: 2002,
      fsgid: 2003,
      ..Default::default()
    };
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
    let mut event = fd_event {
      ino: 123,
      path_id: 1,
      uses_d_dname: 1,
      ..Default::default()
    };

    let mut paths: HashMap<i32, Path> = HashMap::new();
    paths.insert(
      1,
      Path {
        is_absolute: false,
        segments: vec![OutputMsg::Ok(cached_string("eventpoll".to_string()))],
        error: None,
      },
    );

    let pipe = process_path(&event, "pipefs", &paths);
    assert_eq!(pipe.as_ref(), "pipe:[123]");

    let sock = process_path(&event, "sockfs", &paths);
    assert_eq!(sock.as_ref(), "socket:[123]");

    let anon = process_path(&event, "anon_inodefs", &paths);
    assert_eq!(anon.as_ref(), "anon_inode:eventpoll");

    let pidfd = process_path(&event, "pidfs", &paths);
    assert_eq!(pidfd.as_ref(), "anon_inode:[pidfd]");

    event.pseudo_name[..4].copy_from_slice(b"mnt\0");
    let ns = process_path(&event, "nsfs", &paths);
    assert_eq!(ns.as_ref(), "mnt:[123]");

    event.pseudo_name[0] = 0;
    let dmabuf = process_path(&event, "dmabuf", &paths);
    assert_eq!(dmabuf.as_ref(), "/eventpoll:");
    assert!(matches!(dmabuf, OutputMsg::PartialOk(_)));

    let simple = process_path(&event, "aio", &paths);
    assert_eq!(simple.as_ref(), "/eventpoll (deleted)");
    assert!(matches!(simple, OutputMsg::Ok(_)));

    paths.insert(
      1,
      Path {
        is_absolute: true,
        segments: vec![
          OutputMsg::Ok(cached_string("bin".to_string())),
          OutputMsg::Ok(cached_string("usr".to_string())),
        ],
        error: None,
      },
    );
    event.uses_d_dname = 0;
    let normal = process_path(&event, "ext4", &paths);
    assert_eq!(normal.as_ref(), "/usr/bin");
    assert!(matches!(normal, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_process_path_missing_path_is_bpf_error() {
    let mut event = fd_event {
      path_id: 99,
      ..Default::default()
    };
    let paths: HashMap<i32, Path> = HashMap::new();

    let normal = process_path(&event, "ext4", &paths);
    assert!(matches!(
      normal,
      OutputMsg::Err(FriendlyError::Bpf(BpfError::Dropped))
    ));

    let anon = process_path(&event, "anon_inodefs", &paths);
    assert!(matches!(
      anon,
      OutputMsg::Err(FriendlyError::Bpf(BpfError::Dropped))
    ));

    event.uses_d_dname = 1;
    let anon = process_path(&event, "anon_inodefs", &paths);
    assert!(matches!(
      anon,
      OutputMsg::Err(FriendlyError::Bpf(BpfError::Dropped))
    ));

    let ns = process_path(&event, "nsfs", &paths);
    assert!(matches!(
      ns,
      OutputMsg::Err(FriendlyError::Bpf(BpfError::Dropped))
    ));
  }

  #[test]
  fn test_parse_string_event_ok() {
    let header = tracexec_event_header {
      pid: 0,
      flags: 0,
      eid: 0,
      id: 0,
      r#type: crate::bpf::skel::types::event_type::STRING_EVENT,
    };
    let header_len = size_of::<tracexec_event_header>();
    let mut data = vec![0u8; header_len + 6];
    unsafe {
      std::ptr::copy_nonoverlapping(
        &header as *const _ as *const u8,
        data.as_mut_ptr(),
        header_len,
      );
    }
    data[header_len..header_len + 6].copy_from_slice(b"hello\0");
    let msg = parse_string_event(&header, &data);
    assert_eq!(msg.as_ref(), "hello");
    assert!(matches!(msg, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_parse_string_event_error_flag() {
    let header = tracexec_event_header {
      pid: 0,
      flags: BpfEventFlags::STR_READ_FAILURE as u32,
      eid: 0,
      id: 0,
      r#type: crate::bpf::skel::types::event_type::STRING_EVENT,
    };
    let header_len = size_of::<tracexec_event_header>();
    let data = vec![0u8; header_len];
    let msg = parse_string_event(&header, &data);
    assert!(matches!(msg, OutputMsg::Err(FriendlyError::Bpf(_))));
  }

  #[test]
  fn test_parse_path_segment_event_ok() {
    let mut event = path_segment_event::default();
    event.segment[..4].copy_from_slice(b"bin\0");
    let data: &[u8] = unsafe {
      std::slice::from_raw_parts(
        &event as *const _ as *const u8,
        size_of::<path_segment_event>(),
      )
    };
    let msg = parse_path_segment(data);
    assert_eq!(msg.as_ref(), "bin");
    assert!(matches!(msg, OutputMsg::Ok(_)));
  }

  #[test]
  fn test_parse_path_segment_event_error_flag() {
    let mut event = path_segment_event::default();
    event.header.flags = BpfEventFlags::PTR_READ_FAILURE as u32;
    event.segment[..4].copy_from_slice(b"bin\0");
    let data: &[u8] = unsafe {
      std::slice::from_raw_parts(
        &event as *const _ as *const u8,
        size_of::<path_segment_event>(),
      )
    };
    let msg = parse_path_segment(data);
    assert!(matches!(msg, OutputMsg::Err(FriendlyError::Bpf(_))));
  }

  #[test]
  fn test_parse_groups_event_ok() {
    let groups: [nix::libc::gid_t; 2] = [10, 20];
    let header_len = size_of::<tracexec_event_header>();
    let mut data = vec![0u8; header_len + std::mem::size_of_val(&groups)];
    let groups_bytes = bytemuck::cast_slice(&groups);
    data[header_len..].copy_from_slice(groups_bytes);
    let parsed = parse_groups_event(&data);
    assert_eq!(parsed, groups);
  }
}
