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
// This limit can be bypass-ed by using relative paths and the *_at syscall.
#define PATH_MAX 4096
// We set a practical limit for path length
#define PATH_LEN_MAX 65536
// In theory the path depth is unlimited
#define PATH_DEPTH_MAX 65536
// The maximum length of a single segment in the path
// aka NAME_MAX in limits.h
#define PATH_SEGMENT_MAX 256
// Linux doesn't seem to have a limit on fstype name length
#define FSTYPE_NAME_MAX 256

#define BITS_PER_LONG 64
#define NOFILE_MAX 2147483584
// ((NOFILE_MAX) / (BITS_PER_LONG)) = 33554431. it is still too large for
// bpf_loop 1 << 23 = 8388608 is the bpf loop limit. This will take 64MiB space
// per cpu, which is probably too big. Let's set this limit to 2MiB and wait to
// see if anyone complains.
#define FDSET_SIZE_MAX_BYTES 2097152
#define FDSET_SIZE_MAX_IN_LONG ((2097152) / sizeof(long))

// Copy the content to interface.rs after modification!
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
  // A marker for dropped events. This flag is only set in userspace.
  USERSPACE_DROP_MARKER = 1024,
  // Operation stopped early because of errors
  BAIL_OUT = 2048,
  // bpf_loop failure
  LOOP_FAIL = 4096,
  // Failed to read whole path
  PATH_READ_ERR = 8192,
  // inode read failure
  INO_READ_ERR = 16384,
  // mount id read failure
  MNTID_READ_ERR = 32768,
  // filename read failure
  FILENAME_READ_ERR = 65536,
  // file->pos read failure
  POS_READ_ERR = 131072
};

enum event_type {
  SYSENTER_EVENT,
  SYSEXIT_EVENT,
  STRING_EVENT,
  FD_EVENT,
  PATH_SEGMENT_EVENT,
  PATH_EVENT,
  EXIT_EVENT,
  FORK_EVENT,
};

struct tracexec_event_header {
  pid_t pid;
  u32 flags;
  // Globally unique counter of events
  u64 eid;
  // Local counter to detect event drop
  u32 id;
  enum event_type type;
};

struct exec_event {
  struct tracexec_event_header header;
  pid_t tgid;
  uid_t uid;
  uid_t gid;
  s32 syscall_nr;
  s64 ret;
  // argc and env count
  u32 count[2];
  u32 fd_count;
  u32 path_count;
  s32 fd;
  s32 cwd_path_id;
  u64 flags;
  u8 base_filename[PATH_MAX];
  u8 comm[TASK_COMM_LEN];
};

struct string_event {
  struct tracexec_event_header header;
  u8 data[_SC_ARG_MAX];
};

struct fd_event {
  struct tracexec_event_header header;
  unsigned int flags;
  unsigned int fd;
  int mnt_id;
  s32 path_id;
  long unsigned int ino;
  loff_t pos;
  u8 fstype[FSTYPE_NAME_MAX];
};

struct path_event {
  // id: A locally(w.r.t an event) unique counter of path events
  struct tracexec_event_header header;
  u32 segment_count;
};

struct path_segment_event {
  // id: index of this segment
  struct tracexec_event_header header;
  u32 index;
  u8 segment[PATH_SEGMENT_MAX];
};

struct fork_event {
  struct tracexec_event_header header;
  pid_t parent_tgid;
  // pid_t new_tgid; stored in header->pid
};

struct exit_event {
  struct tracexec_event_header header;
  int code;
  u32 sig;
  bool is_root_tracee;
};

union cache_item {
  struct string_event string;
  struct fd_event fd;
  struct path_event path;
  struct path_segment_event segment;
  struct fork_event fork;
  struct exit_event exit;
};
#endif
