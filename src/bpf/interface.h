// This file must be kept in sync with interface.rs to ensure ABI compatibility

#ifndef __INTERFACE_H__
#define __INTERFACE_H__

#include "vmlinux.h"

// Length and count limit on argv, assuming page size is 4096:
// https://elixir.bootlin.com/linux/v6.11-rc2/source/include/uapi/linux/binfmts.h
#define PAGE_SIZE 4096
#define KERNEL_MAX_ARG_STRLEN (PAGE_SIZE * 32)
#define KERNEL_MAX_ARG_STRINGS 0x7FFFFFFF

// The limit for argc + argv + envp
// getconf ARG_MAX
// TODO: determine it at runtime
#define _SC_ARG_MAX 2097152
// The maximum possible value of argc and num of env, used in loop
// ceil(ARG_MAX/9)
//   each pointer takes 8 bytes and each arg contains at least one NUL byte
#define ARGC_MAX 233017

// The limit for filename
// https://elixir.bootlin.com/linux/v6.10.3/source/include/uapi/linux/limits.h#L13
#define PATH_MAX 4096

enum exec_event_flags {
  // This flag is set if any other error occurs
  ERROR = 1,
  // This flag is set if we don't have enough loops to read all items
  TOO_MANY_ITEMS = 2,
  COMM_READ_FAILURE = 4,
  POSSIBLE_TRUNCATION = 8,
  PTR_READ_FAILURE = 16,
  NO_ROOM = 32,
  STR_READ_FAILURE = 64,
};

struct exec_event {
  pid_t pid;
  pid_t ppid;
  uid_t uid;
  uid_t gid;
  u32 envc;
  // argc and env count
  u32 count[2];
  u32 flags;
  // u32 gap;
  // Globally unique counter of events
  u64 eid;
  char filename[PATH_MAX];
  char comm[TASK_COMM_LEN];
};

struct string_entry {
  pid_t pid;
  u32 flags;
  u64 eid;
  char data[_SC_ARG_MAX];
};
#endif