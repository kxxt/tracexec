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