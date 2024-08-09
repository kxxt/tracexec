#include "common.h"
#include "interface.h"
#include <bpf/bpf_core_read.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

static const struct exec_event empty_event = {};

static u32 drop_counter = 0;

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 1024);
  __type(key, pid_t);
  __type(value, struct exec_event);
} execs SEC(".maps");

struct reader_context {
  struct exec_event *event;
  // Points to the first unused byte in event->data
  u32 tail;
  u8 **ptr;
};

static int read_argv(u32 index, struct reader_context *ctx);

#define debug(...) bpf_printk("tracexec_system: " __VA_ARGS__);

SEC("tracepoint/syscalls/sys_enter_execve")
int tp_sys_enter_execve(struct sys_enter_execve_args *ctx) {
  struct task_struct *task;
  struct exec_event *event;
  // Collect UID/GID information
  uid_t uid, gid;
  u64 tmp = bpf_get_current_uid_gid();
  uid = (uid_t)tmp;
  gid = tmp >> 32;
  // Collect pid/tgid information
  pid_t pid, tgid;
  tmp = bpf_get_current_pid_tgid();
  pid = (pid_t)tmp;
  tgid = tmp >> 32;
  // Create event
  if (bpf_map_update_elem(&execs, &pid, &empty_event, BPF_NOEXIST)) {
    // Cannot allocate new event, map is full!
    bpf_printk("tracexec_system: Failed to allocate new event!");
    drop_counter++;
    return 0;
  }
  event = bpf_map_lookup_elem(&execs, &pid);
  if (!event)
    return 0;
  // Read comm
  if (0 != bpf_get_current_comm(event->comm, sizeof(event->comm))) {
    // Failed to read comm
    event->comm[0] = '\0';
    event->flags |= COMM_READ_FAILURE;
  };
  // Read filename
  if (bpf_probe_read_user_str(event->filename, sizeof(event->filename),
                              ctx->filename) == sizeof(event->filename)) {
    // The filename is possibly truncated, we cannot determine
    event->flags |= FILENAME_POSSIBLE_TRUNCATION;
  }
  bpf_printk("%s execve %s UID: %d GID: %d PID: %d\n", event->comm,
             event->filename, uid, gid, pid);
  // Read argv
  struct reader_context reader_ctx;
  reader_ctx.event = event;
  reader_ctx.ptr = ctx->argv;
  reader_ctx.tail = 0;
  // bpf_loop allows 1 << 23 (~8 million) loops, otherwise we cannot achieve it
  bpf_loop(ARGC_MAX, read_argv, &reader_ctx, 0);
  // Read envp
  // Read file descriptors
  return 0;
}

SEC("tracepoint/syscalls/sys_exit_execve")
int tp_sys_exit_execve(struct sys_exit_exec_args *ctx) {
  pid_t pid = (pid_t)bpf_get_current_pid_tgid();
  bpf_printk("execve result: %d PID %d\n", ctx->ret, pid);
  if (0 != bpf_map_delete_elem(&execs, &pid)) {
    bpf_printk("Failed to del element from execs map");
  }
  return 0;
}

static int read_argv(u32 index, struct reader_context *ctx) {
  struct exec_event *event = ctx->event;
  const u8 *argp = NULL;
  int ret = bpf_probe_read_user(&argp, sizeof(argp), &ctx->ptr[index]);
  if (ret < 0) {
    event->flags |= ARG_PTR_READ_FAILURE;
    debug("Failed to read pointer to arg");
    return 1;
  }
  if (argp == NULL) {
    // We have reached the end of argv
    event->argc = index;
    return 1;
  }
  // Read the str into data
  s64 rest = (s64)sizeof(event->data) - (s64)ctx->tail;
  if (rest <= 0) {
    event->flags |= NO_ROOM_FOR_ARGS;
    event->argc = index - 1;
    return 1;
  }
  if (ctx->tail >= sizeof(event->data)) {
    // This is not going to happen. Just make the verifier happy.
    return 1;
  }
  void *start = &event->data[ctx->tail];
  if (start >= (void *)&event->data[_SC_ARG_MAX - 1]) {
    return 1;
  }
  // The verifier assumes that start is a variable in range [0, 2101291]
  // and rest in range [0, 2101292], so it rejects access to ptr (start + rest)
  // where start = 2101291, rest = 2101292
  // But in practice, rest is bounded by [0, end - start].
  // The verifier seems unable to reason about variables in ranges.
  s64 bytes_read = bpf_probe_read_user_str(start, (u32)rest, argp);
  if (bytes_read < 0) {
    debug("failed to read arg %d(addr:%x) from userspace", index, argp);
    event->flags |= ARG_READ_FAILURE;
    // Replace such args with '\0'
    if (ctx->tail > sizeof(event->data)) {
      // This is not going to happen. Just make the verifier happy.
      return 1;
    }
    event->data[ctx->tail] = '\0';
    ++ctx->tail;
    // continue
    return 0;
  } else if (bytes_read == rest) {
    event->flags |= ARG_POSSIBLE_TRUNCATION;
  }
  ctx->tail += bytes_read;
  event->argc = index + 1;

  if (index == ARGC_MAX - 1) {
    // We hit ARGC_MAX
    // We are not going to iterate further.
    event->flags |= TOO_MANY_ARGS;
  }
  return 0;
}