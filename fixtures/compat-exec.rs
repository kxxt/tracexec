//! 64-bit helper that issues 32-bit execve/execveat syscalls via int 0x80.
//! Used by tests to trigger compat_sys_execve* paths.

#[cfg(target_arch = "x86_64")]
use std::{
  ffi::CString,
  ptr::null_mut,
};

#[cfg(target_arch = "x86_64")]
use nix::libc::{
  self,
  c_int,
};

#[cfg(target_arch = "x86_64")]
fn map_32(size: usize) -> *mut u8 {
  unsafe {
    let ptr = libc::mmap(
      null_mut(),
      size,
      libc::PROT_READ | libc::PROT_WRITE,
      libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_32BIT,
      -1,
      0,
    );
    if ptr == libc::MAP_FAILED {
      libc::_exit(111);
    }
    ptr as *mut u8
  }
}

#[cfg(target_arch = "x86_64")]
unsafe fn write_cstr(base: *mut u8, s: &CString) -> *mut libc::c_char {
  let bytes = s.as_bytes_with_nul();
  unsafe {
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), base, bytes.len());
  }
  base as *mut libc::c_char
}

#[cfg(not(target_arch = "x86_64"))]
fn main() {
  eprintln!("compat-exec is only supported on x86_64");
  std::process::exit(1);
}

#[cfg(target_arch = "x86_64")]
fn main() {
  let args: Vec<String> = std::env::args().collect();
  if args.iter().any(|arg| arg == "stop") {
    return;
  }
  let mode_execveat = args.iter().any(|arg| arg == "execveat");
  let target = CString::new("/proc/self/exe").unwrap();
  let arg0 = CString::new("stop").unwrap();

  // Layout in MAP_32BIT region: filename, argv array, envp array
  let buf = map_32(4096);
  unsafe {
    let filename_ptr = write_cstr(buf, &target);
    let argv_ptr = buf.add(256) as *mut *const libc::c_char;
    *argv_ptr = write_cstr(buf.add(512), &arg0);
    *argv_ptr.add(1) = std::ptr::null();
    let envp_ptr = buf.add(768) as *mut *const libc::c_char;
    *envp_ptr = std::ptr::null();

    let filename_u32 = filename_ptr as usize as u32;
    let argv_u32 = argv_ptr as usize as u32;
    let envp_u32 = envp_ptr as usize as u32;

    let mut ret: c_int;
    if mode_execveat {
      // 32-bit __NR_execveat = 358
      core::arch::asm!(
        "push rbx",
        "mov ebx, {fd:e}",
        "int 0x80",
        "pop rbx",
        in("eax") 358u32,
        fd = in(reg) libc::AT_FDCWD as u32,
        in("ecx") filename_u32,
        in("edx") argv_u32,
        in("esi") envp_u32,
        in("edi") 0u32,
        lateout("eax") ret,
      );
    } else {
      // 32-bit __NR_execve = 11
      core::arch::asm!(
        "push rbx",
        "mov ebx, {filename:e}",
        "int 0x80",
        "pop rbx",
        in("eax") 11u32,
        filename = in(reg) filename_u32,
        in("ecx") argv_u32,
        in("edx") envp_u32,
        lateout("eax") ret,
      );
    }
    libc::_exit(ret);
  }
}
