#include "common.h"
#include "interface.h"
#include <bpf/bpf_core_read.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

static const struct exec_event empty_event = {};
static u64 event_counter = 0;
static u32 drop_counter = 0;

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 1024);
  __type(key, pid_t);
  __type(value, struct exec_event);
} execs SEC(".maps");

// A staging area for writing variable length strings
// I cannot really use a percpu array due to size limit:
// https://github.com/iovisor/bcc/issues/2519
struct {
  __uint(type, BPF_MAP_TYPE_ARRAY);
  __uint(max_entries, MAX_CPUS); // TODO: Can we change this at load time?
  __type(key, u32);
  __type(value, struct string_event);
} cache SEC(".maps");
// tracing progs cannot use bpf_spin_lock yet
// static struct bpf_spin_lock cache_lock;

// This string_io map is used to send variable length cstrings back to
// userspace. We cannot simply write all cstrings into one single fixed buffer
// because it's hard to make verifier happy. (PRs are welcome if that could be
// done) (TODO: check if this could be done with dynptr)
struct {
  __uint(type, BPF_MAP_TYPE_RINGBUF);
  __uint(max_entries,
         134217728); // TODO: determine a good size for ringbuf, 128MiB for now
} events SEC(".maps");

struct reader_context {
  struct exec_event *event;
  // index:
  // 0: arg
  // 1: envp
  u32 index;
  // ptr is a userspace pointer to an array of cstring pointers
  u8 **ptr;
};

static int read_strings(u32 index, struct reader_context *ctx);

#ifdef EBPF_DEBUG
#define debug(...) bpf_printk("tracexec_system: " __VA_ARGS__);
#else
#define debug(...)
#endif

int trace_exec_common(struct sys_enter_exec_args *ctx) {
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
    debug("Failed to allocate new event!");
    drop_counter++;
    return 0;
  }
  struct exec_event *event = bpf_map_lookup_elem(&execs, &pid);
  if (!event || !ctx)
    return 0;
  // Initialize event
  event->header.pid = pid;
  event->header.type = SYSEXIT_EVENT;
  event->header.eid = __sync_fetch_and_add(&event_counter, 1);
  // Read comm
  if (0 != bpf_get_current_comm(event->comm, sizeof(event->comm))) {
    // Failed to read comm
    event->comm[0] = '\0';
    event->header.flags |= COMM_READ_FAILURE;
  };
  // Read base filename
  if (ctx->base_filename == NULL) {
    debug("filename is NULL");
    event->base_filename[0] = '\0';
  } else if (bpf_probe_read_user_str(
                 event->base_filename, sizeof(event->base_filename),
                 ctx->base_filename) == sizeof(event->base_filename)) {
    // The filename is possibly truncated, we cannot determine
    event->header.flags |= POSSIBLE_TRUNCATION;
  }
  debug("%ld %s execve %s UID: %d GID: %d PID: %d\n", event->header.eid,
        event->comm, event->base_filename, uid, gid, pid);
  // Read argv
  struct reader_context reader_ctx;
  reader_ctx.event = event;
  reader_ctx.ptr = ctx->argv;
  reader_ctx.index = 0;
  // bpf_loop allows 1 << 23 (~8 million) loops, otherwise we cannot achieve it
  bpf_loop(ARGC_MAX, read_strings, &reader_ctx, 0);
  // Read envp
  reader_ctx.ptr = ctx->envp;
  reader_ctx.index = 1;
  bpf_loop(ARGC_MAX, read_strings, &reader_ctx, 0);
  return 0;
}

SEC("tracepoint/syscalls/sys_enter_execve")
int tp_sys_enter_execve(struct sys_enter_execve_args *ctx) {
  struct task_struct *task;
  struct exec_event *event;
  struct sys_enter_exec_args common_ctx = {.__syscall_nr = ctx->__syscall_nr,
                                           .argv = ctx->argv,
                                           .envp = ctx->envp,
                                           .base_filename = ctx->filename};
  trace_exec_common(&common_ctx);
  // Read file descriptors
  return 0;
}

SEC("tracepoint/syscalls/sys_exit_execve")
int tp_sys_exit_execve(struct sys_exit_exec_args *ctx) {
  pid_t pid = (pid_t)bpf_get_current_pid_tgid();
  struct exec_event *event;
  event = bpf_map_lookup_elem(&execs, &pid);
  if (event == NULL) {
    debug("Failed to lookup exec_event on sysexit");
    drop_counter += 1;
    return 0;
  }
  event->ret = ctx->ret;
  event->header.type = SYSEXIT_EVENT;
  debug("execve result: %d PID %d\n", ctx->ret, pid);
  long ret = bpf_ringbuf_output(&events, event, sizeof(struct exec_event), 0);
  if (ret != 0) {
#ifdef EBPF_DEBUG
    u64 avail = bpf_ringbuf_query(&events, BPF_RB_AVAIL_DATA);
    debug("Failed to write exec event to ringbuf: %d, avail: %lu", ret, avail);
#endif
  }
  if (0 != bpf_map_delete_elem(&execs, &pid)) {
    debug("Failed to del element from execs map");
  }
  return 0;
}

static int read_strings(u32 index, struct reader_context *ctx) {
  struct exec_event *event = ctx->event;
  const u8 *argp = NULL;
  int ret = bpf_probe_read_user(&argp, sizeof(argp), &ctx->ptr[index]);
  if (ret < 0) {
    event->header.flags |= PTR_READ_FAILURE;
    debug("Failed to read pointer to arg");
    return 1;
  }
  if (argp == NULL) {
    // We have reached the end of argv
    event->count[ctx->index] = index;
    return 1;
  }
  // Read the str into a temporary buffer
  u32 entry_index = bpf_get_smp_processor_id();
  if (entry_index > MAX_CPUS) {
    debug("Too many cores!");
    return 1;
  }
  struct string_event *entry = bpf_map_lookup_elem(&cache, &entry_index);
  if (entry == NULL) {
    debug("This should not happen!");
    return 1;
  }
  entry->header.type = STRING_EVENT;
  entry->header.pid = event->header.pid;
  entry->header.eid = event->header.eid;
  s64 bytes_read =
      bpf_probe_read_user_str(entry->data, sizeof(entry->data), argp);
  if (bytes_read < 0) {
    debug("failed to read arg %d(addr:%x) from userspace", index, argp);
    entry->header.flags |= STR_READ_FAILURE;
    // Replace such args with '\0'
    entry->data[0] = '\0';
    bytes_read = 1;
    event->count[ctx->index] = index + 1;
    // continue
    return 0;
  } else if (bytes_read == sizeof(entry->data)) {
    entry->header.flags |= POSSIBLE_TRUNCATION;
  }
  bpf_ringbuf_output(&events, entry, sizeof(struct event_header) + bytes_read,
                     0);
  event->count[ctx->index] = index + 1;
  if (index == ARGC_MAX - 1) {
    // We hit ARGC_MAX
    // We are not going to iterate further.
    // Note that TOO_MANY_ITEMS flag is set on event instead of string entry.
    event->header.flags |= TOO_MANY_ITEMS;
  }
  return 0;
}