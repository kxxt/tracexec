#include "common.h"
#include "interface.h"
#include <bpf/bpf_core_read.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <errno.h>

char LICENSE[] SEC("license") = "GPL";

static const struct exec_event empty_event = {};
static u64 event_counter = 0;
static u32 drop_counter = 0;
const volatile struct {
  u32 max_num_cpus;
  u32 nofile;
} config = {.max_num_cpus = MAX_CPUS,
            // https://www.kxxt.dev/blog/max-possible-value-of-rlimit-nofile/
            .nofile = 2147483584};

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
  __uint(max_entries, MAX_CPUS);
  __type(key, u32);
  __type(value, union cache_item);
} cache SEC(".maps");
// tracing progs cannot use bpf_spin_lock yet
// static struct bpf_spin_lock cache_lock;

// This string_io map is used to send variable length cstrings back to
// userspace. We cannot simply write all cstrings into one single fixed buffer
// because it's hard to make verifier happy. (PRs are welcome if that could be
// done) (TODO: check if this could be done with dynptr)
struct {
  __uint(type, BPF_MAP_TYPE_RINGBUF);
  // Every exec event takes up to 2MiB space for argc+argv+envp,
  // so on a machine with 64 cores, there can be at most 64 execs happening in
  // parallel, taking at most 128MiB space in a burst. We haven't considered the
  // rate at which the userspace code consumes event, 256MiB is used as a
  // heruistic for now
  __uint(max_entries, 268435456);
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

struct fdset_reader_context {
  struct exec_event *event;
  struct file *fd_array;
  long *fdset;
  unsigned int size;
};

static int read_strings(u32 index, struct reader_context *ctx);
static int read_fds(struct exec_event *event);
static int read_fds_impl(u32 index, struct fdset_reader_context *ctx);
static int read_fd(unsigned int fd_num, struct file *fd_array,
                   struct exec_event *event);

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
  // Read file descriptors
  read_fds(event);
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

// Collect information about file descriptors of the process on sysenter of exec
static int read_fds(struct exec_event *event) {
  if (event == NULL)
    return 1;
  struct task_struct *current = (struct task_struct *)bpf_get_current_task();
  struct files_struct *files;
  int ret;
  ret = bpf_core_read(&files, sizeof(void *), &current->files);
  if (ret < 0) {
    debug("Failed to read current->files! err: %d", ret);
    goto probe_failure;
  }
  // Accessing fdt usually requires RCU. Is it okay to access without it in BPF?
  // bpf_rcu_read_lock is a kfunc anyway.
  // https://docs.kernel.org/filesystems/files.html
  // files_fdtable() uses rcu_dereference() macro which takes care of the memory
  // barrier requirements for lock-free dereference. The fdtable pointer must be
  // read within the read-side critical section.
  struct fdtable *fdt;
  bpf_rcu_read_lock();
  ret = bpf_core_read(&fdt, sizeof(void *), &files->fdt);
  if (ret < 0) {
    debug("Failed to read files->fdt! err: %d", ret);
    goto probe_failure_locked_rcu;
  }
  struct file *fd_array;
  ret = bpf_core_read(&fd_array, sizeof(void *), &fdt->fd);
  if (ret < 0) {
    debug("Failed to read fdt->fd! err: %d", ret);
    goto probe_failure_locked_rcu;
  }
  long *fdset;
  ret = bpf_core_read(&fdset, sizeof(void *), &fdt->open_fds);
  if (ret < 0) {
    debug("Failed to read fdt->open_fds! err: %d", ret);
    goto probe_failure_locked_rcu;
  }
  unsigned int max_fds;
  // max_fds is 128 or 256 for most processes that does not open too many files
  // max_fds is a multiple of BITS_PER_LONG. TODO: Should we rely on this kernel
  // implementation detail.
  ret = bpf_core_read(&max_fds, sizeof(max_fds), &fdt->max_fds);
  if (ret < 0) {
    debug("Failed to read fdt->max_fds! err: %d", ret);
    goto probe_failure_locked_rcu;
  }
  bpf_rcu_read_unlock();
  // open_fds is a fd set, which is a bitmap
  // Copy it into cache first
  // Ref:
  // https://github.com/torvalds/linux/blob/5189dafa4cf950e675f02ee04b577dfbbad0d9b1/fs/file.c#L279-L291
  unsigned int fdset_size = max_fds / BITS_PER_LONG;
  fdset_size = min(fdset_size, FDSET_SIZE_MAX_IN_LONG);
  struct fdset_reader_context ctx = {
      .event = event,
      .fdset = fdset,
      .fd_array = fd_array,
      .size = fdset_size,
  };
  bpf_loop(fdset_size, read_fds_impl, &ctx, 0);
  return 0;
probe_failure_locked_rcu:
  bpf_rcu_read_unlock();
probe_failure:
  event->header.flags |= FDS_PROBE_FAILURE;
  return -EFAULT;
}

// Ref:
// https://elixir.bootlin.com/linux/v6.10.3/source/include/asm-generic/bitops/__ffs.h#L45
static __always_inline unsigned int generic___ffs(unsigned long word) {
  unsigned int num = 0;

#if BITS_PER_LONG == 64
  if ((word & 0xffffffff) == 0) {
    num += 32;
    word >>= 32;
  }
#endif
  if ((word & 0xffff) == 0) {
    num += 16;
    word >>= 16;
  }
  if ((word & 0xff) == 0) {
    num += 8;
    word >>= 8;
  }
  if ((word & 0xf) == 0) {
    num += 4;
    word >>= 4;
  }
  if ((word & 0x3) == 0) {
    num += 2;
    word >>= 2;
  }
  if ((word & 0x1) == 0)
    num += 1;
  return num;
}

// Find the next set bit
//   Returns the bit number for the next set bit
//   If no bits are set, returns BITS_PER_LONG.
// Ref:
// https://github.com/torvalds/linux/blob/0b2811ba11b04353033237359c9d042eb0cdc1c1/include/linux/find.h#L44-L69
static __always_inline unsigned int find_next_bit(long bitmap,
                                                  unsigned int offset) {
  if (offset > BITS_PER_LONG)
    return BITS_PER_LONG;
  bitmap &= GENMASK(BITS_PER_LONG - 1, offset);
  return bitmap ? generic___ffs(bitmap) : BITS_PER_LONG;
}

// A helper to read fdset into cache,
// read open file descriptors and send info into ringbuf
static int read_fds_impl(u32 index, struct fdset_reader_context *ctx) {
  struct exec_event *event;
  if (ctx == NULL || (event = ctx->event) == NULL)
    return 1; // unreachable
  struct file *fd_array = ctx->fd_array;
  // 64 bits of a larger fdset.
  long *pfdset = &ctx->fdset[index];
  long fdset;
  // Read a 64bits part of fdset from kernel
  int ret = bpf_core_read(&fdset, sizeof(fdset), pfdset);
  if (ret < 0) {
    debug("Failed to read %u/%u member of fdset", index, ctx->size);
    event->header.flags |= FDS_PROBE_FAILURE;
    return 1;
  }
  debug("fdset %u/%u = %lx", index, ctx->size, fdset);
  // if it's all zeros, let's skip it:
  if (fdset == 0)
    return 0;
  unsigned int next_bit = BITS_PER_LONG;
  next_bit = find_next_bit(fdset, 0);
// #pragma unroll
//   for (int i = 0; i < BITS_PER_LONG; i++) {
//     if (next_bit == BITS_PER_LONG)
//       break;
//     unsigned int fdnum = next_bit + BITS_PER_LONG * index;
//     read_fd(fdnum, fd_array, event);
//     next_bit = find_next_bit(fdset, next_bit + 1);
//   }
  return 0;
}

// Gather information about a single fd and send it back to userspace
static int read_fd(unsigned int fd_num, struct file *fd_array,
                   struct exec_event *event) {
  if (event == NULL)
    return 1;
  u32 entry_index = bpf_get_smp_processor_id();
  if (entry_index > config.max_num_cpus) {
    debug("Too many cores!");
    return 1;
  }
  struct fd_event *entry = bpf_map_lookup_elem(&cache, &entry_index);
  if (entry == NULL) {
    debug("This should not happen!");
    return 1;
  }
  entry->header.type = FD_EVENT;
  entry->header.pid = event->header.pid;
  entry->header.eid = event->header.eid;

  struct file *file = &fd_array[fd_num];
  struct path *path;
  int ret = bpf_core_read(&path, sizeof(void *), &file->f_path);
  if (ret < 0) {
    debug("failed to read file->f_path: %d", ret);
    entry->path[0] = '\0';
    goto out;
  }
  // bpf_d_path is not available
  // ret = bpf_d_path(path, (char *)entry->path, PATH_MAX);
  entry->path[0] = '\0';
  debug("open fd: %u -> %s", fd_num, entry->path);
out:
  bpf_ringbuf_output(&events, entry, sizeof(struct fd_event), 0);
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
  if (entry_index > config.max_num_cpus) {
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
    goto out;
  } else if (bytes_read == sizeof(entry->data)) {
    entry->header.flags |= POSSIBLE_TRUNCATION;
  }
out:
  ret = bpf_ringbuf_output(&events, entry,
                           sizeof(struct event_header) + bytes_read, 0);
  if (ret < 0) {
    event->header.flags |= OUTPUT_FAILURE;
  }
  event->count[ctx->index] = index + 1;
  if (index == ARGC_MAX - 1) {
    // We hit ARGC_MAX
    // We are not going to iterate further.
    // Note that TOO_MANY_ITEMS flag is set on event instead of string entry.
    event->header.flags |= TOO_MANY_ITEMS;
  }
  return 0;
}