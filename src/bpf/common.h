#ifndef __COMMON_H__
#define __COMMON_H__

#include "vmlinux.h"

/* BPF cannot access this struct */
struct forbidden_common_args {
  u16 type;
  u8 flags;
  u8 preempt_count;
  s32 pid;
};

struct sys_enter_execve_args {
  struct forbidden_common_args common;
  s32 __syscall_nr;
  u32 pad;
  const u8 *filename;
  const u8 *const *argv;
  const u8 *const *envp;
};

struct sys_enter_execveat_args {
  struct forbidden_common_args common;
  s32 __syscall_nr;
  u64 fd;
  const u8 *filename;
  const u8 *const *argv;
  const u8 *const *envp;
  u64 flags;
};

struct sys_enter_exec_args {
  s32 __syscall_nr;
  const u8 *base_filename;
  const u8 *const *argv;
  const u8 *const *envp;
};

struct sys_exit_exec_args {
  struct forbidden_common_args common;
  s32 __syscall_nr;
  s64 ret;
};

#endif