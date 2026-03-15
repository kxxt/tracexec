//! Parsers for data structures read back from eBPF ringbuf

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
  },
};

use crate::bpf::{
  cached_cow,
  interface::BpfEventFlags,
  skel::types::exec_event,
  utf8_lossy_cow_from_bytes_with_nul,
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
