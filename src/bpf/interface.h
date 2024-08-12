// This header contains the binary interface between ebpf program and userspace
// program

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

#define BITS_PER_LONG 64
#define NOFILE_MAX 2147483584
// ((NOFILE_MAX) / (BITS_PER_LONG)) = 33554431. it is still too large for
// bpf_loop 1 << 23 = 8388608 is the bpf loop limit. This will take 64MiB space
// per cpu, which is probably too big. Let's set this limit to 2MiB and wait to
// see if anyone complains.
#define FDSET_SIZE_MAX_BYTES 2097152
#define FDSET_SIZE_MAX_IN_LONG ((2097152) / sizeof(long))

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
  // Failed to get information about fds
  FDS_PROBE_FAILURE = 128,
  // Failed to send event into ringbuf
  OUTPUT_FAILURE = 256,
  // Failed to read flags
  FLAGS_READ_FAILURE = 512,
};

enum event_type {
  SYSENTER_EVENT,
  SYSEXIT_EVENT,
  STRING_EVENT,
  FD_EVENT,
};

struct event_header {
  pid_t pid;
  u32 flags;
  // Globally unique counter of events
  u64 eid;
  enum event_type type;
};

struct exec_event {
  struct event_header header;
  pid_t ppid;
  uid_t uid;
  uid_t gid;
  s64 ret;
  // argc and env count
  u32 count[2];
  u8 base_filename[PATH_MAX];
  u8 filename[PATH_MAX];
  u8 comm[TASK_COMM_LEN];
};

struct string_event {
  struct event_header header;
  u8 data[_SC_ARG_MAX];
};

struct fd_event {
  struct event_header header;
  unsigned int flags;
  u8 path[PATH_MAX];
};

union cache_item {
  struct string_event string;
  struct fd_event fd;
};
#endif