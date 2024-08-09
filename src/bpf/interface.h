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
// The maximum possible value of argc, used in loop
// ceil(ARG_MAX/9)
//   each pointer takes 8 bytes and each arg contains at least one NUL byte
#define ARGC_MAX 233017

// The limit for filename
// https://elixir.bootlin.com/linux/v6.10.3/source/include/uapi/linux/limits.h#L13
#define PATH_MAX 4096

enum exec_event_flags {
  // This flag is set if any other error occurs
  ERROR = 1,
  // This flag is set if we don't have enough loops to read argv
  TOO_MANY_ARGS = 2,
  // This flag is set if we don't have enough loops to read envp
  TOO_MANY_ENVS = 4,
  COMM_READ_FAILURE = 8,
  FILENAME_POSSIBLE_TRUNCATION = 16,
  ARG_PTR_READ_FAILURE = 32,
  ENV_PTR_READ_FAILURE = 64,
  NO_ROOM_FOR_ARGS = 128,
  NO_ROOM_FOR_ENVS = 256,
  ARG_READ_FAILURE = 512,
  ENV_READ_FAILURE = 1024,
  ARG_POSSIBLE_TRUNCATION = 2048,
  ENV_POSSIBLE_TRUNCATION = 4096,
};

struct exec_event {
  pid_t pid;
  pid_t ppid;
  uid_t uid;
  uid_t gid;
  u32 envc;
  u32 argc; // KERNEL_MAX_ARG_STRINGS fits in s32
  u32 flags;
  char filename[PATH_MAX];
  char comm[TASK_COMM_LEN];
  char data[_SC_ARG_MAX]; // NULL separated argv, NULL, and NULL separated envp
  // u8 unused;
};
#endif