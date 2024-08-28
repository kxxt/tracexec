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
  bool follow_fork;
  pid_t tracee_pid;
  unsigned int tracee_pidns_inum;
} config = {
    .max_num_cpus = MAX_CPUS,
    // https://www.kxxt.dev/blog/max-possible-value-of-rlimit-nofile/
    .nofile = 2147483584,
    .follow_fork = false,
    .tracee_pid = 0,
    .tracee_pidns_inum = 0,
};

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  __uint(max_entries, 1024);
  __type(key, pid_t);
  __type(value, struct exec_event);
} execs SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_HASH);
  // 2^22 = 4194304, max number of pid on x86_64
  // Some pids cannot be used (like pid 0)
  __uint(max_entries, 4194303);
  __type(key, pid_t);
  // The value is useless. We just use this map as a hash set.
  __type(value, char);
} tgid_closure SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
  __type(key, u32);
  __type(value, struct path_segment_event);
  __uint(max_entries, 1);
} path_segment_cache SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
  __type(key, u32);
  __type(value, struct path_event);
  __uint(max_entries, 1);
} path_event_cache SEC(".maps");

// A staging area for writing variable length strings
// I cannot really use a percpu array due to size limit:
// https://github.com/iovisor/bcc/issues/2519
struct {
  __uint(type, BPF_MAP_TYPE_ARRAY);
  __uint(max_entries, MAX_CPUS);
  __type(key, u32);
  __type(value, union cache_item);
} cache SEC(".maps");

struct {
  __uint(type, BPF_MAP_TYPE_RINGBUF);
  // Every exec event takes up to 2MiB space for argc+argv+envp, (without
  // considering the space taken by path segments) so on a machine with 64
  // cores, there can be at most 64 execs happening in parallel, taking at most
  // 128MiB space in a burst. We haven't considered the rate at which the
  // userspace code consumes event, 256MiB is used as a heruistic for now
  __uint(max_entries, 268435456);
} events SEC(".maps");

struct reader_context {
  struct exec_event *event;
  // index:
  // 0: arg
  // 1: envp
  u32 index;
  // ptr is a userspace pointer to an array of cstring pointers
  const u8 *const *ptr;
};

struct fdset_reader_context {
  struct exec_event *event;
  struct file **fd_array;
  long *fdset;
  unsigned int size;
};

struct fdset_word_reader_context {
  struct exec_event *event;
  struct file **fd_array;
  long fdset;
  unsigned int next_bit;
  unsigned int word_index;
};

static int read_strings(u32 index, struct reader_context *ctx);
static int read_fds(struct exec_event *event);
static int read_fds_impl(u32 index, struct fdset_reader_context *ctx);
static int read_fdset_word(u32 index, struct fdset_word_reader_context *ctx);
static int _read_fd(unsigned int fd_num, struct file **fd_array,
                    struct exec_event *event);
static int add_tgid_to_closure(pid_t tgid);
static int read_send_path(const struct path *path,
                          const struct event_header *base_header, s32 path_id);

#ifdef EBPF_DEBUG
#define debug(...) bpf_printk("tracexec_system: " __VA_ARGS__);
#else
#define debug(...)
#endif

bool should_trace(pid_t old_tgid) {
  // Trace all if not following forks
  if (!config.follow_fork)
    return true;
  // Check if it is in the closure
  void *ptr = bpf_map_lookup_elem(&tgid_closure, &old_tgid);
  if (ptr != NULL)
    return true;
  // config.tracee_pid might not be in init pid ns,
  // thus we cannot simply compare tgid and config.tracee_pid
  // Here we solve it by comparing tgid and the inode number of pid namespace
  struct task_struct *task = (void *)bpf_get_current_task();
  struct nsproxy *nsproxy;
  struct pid *pid_struct;
  int ret = bpf_core_read(&nsproxy, sizeof(void *), &task->nsproxy);
  if (ret < 0) {
    debug("failed to read nsproxy struct: %d", ret);
    return false;
  }
  // RCU read lock when accessing the active pid ns,
  // ref: https://elixir.bootlin.com/linux/v6.11-rc4/source/kernel/pid.c#L505
  bpf_rcu_read_lock();
  ret = bpf_core_read(&pid_struct, sizeof(void *), &task->thread_pid);
  if (ret < 0) {
    debug("failed to read task->thread_pid: %d", ret);
    goto err_unlock;
  }
  int level;
  ret = bpf_core_read(&level, sizeof(int), &pid_struct->level);
  if (ret < 0) {
    debug("failed to read pid->level: %d", ret);
    goto err_unlock;
  }
  // ref: ns_of_pid
  // https://elixir.bootlin.com/linux/v6.11-rc4/source/include/linux/pid.h#L145
  struct upid upid;
  ret = bpf_core_read(&upid, sizeof(struct upid), &pid_struct->numbers[level]);
  if (ret < 0) {
    debug("failed to read pid->numbers[level]: %d", ret);
    goto err_unlock;
  }
  pid_t pid_in_ns = upid.nr;
  struct pid_namespace *pid_ns = upid.ns;
  // inode number of this pid_ns
  unsigned int ns_inum;
  ret = bpf_core_read(&ns_inum, sizeof(unsigned int), &pid_ns->ns.inum);
  if (ret < 0) {
    debug("failed to read pid_ns->ns.inum: %d", ret);
    goto err_unlock;
  }
  bpf_rcu_read_unlock();
  if (pid_in_ns == config.tracee_pid && ns_inum == config.tracee_pidns_inum) {
    debug("TASK %d (%d in pidns %u) is tracee", old_tgid, pid_in_ns, ns_inum);
    // Add it to the closure to avoid hitting this slow path in the future
    add_tgid_to_closure(old_tgid);
    return true;
  }
  return false;
err_unlock:
  bpf_rcu_read_unlock();
  return false;
}

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
  // debug("sysenter: pid=%d, tgid=%d, tracee=%d", pid, tgid,
  // config.tracee_pid); Create event
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
  event->tgid = tgid;
  // Initialize the event even if we don't really trace it.
  // This way we have access to old tgid on exec sysexit so that
  // we are also able to check it on exec sysexit
  if (!should_trace(tgid))
    return 0;
  event->header.type = SYSEXIT_EVENT;
  event->header.eid = __sync_fetch_and_add(&event_counter, 1);
  event->count[0] = event->count[1] = event->fd_count = event->path_count = 0;
  event->syscall_nr = ctx->syscall_nr;
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

SEC("tp_btf/sched_process_fork")
int trace_fork(u64 *ctx) {
  if (!config.follow_fork)
    return 0;
  struct task_struct *parent = (struct task_struct *)ctx[0];
  struct task_struct *child = (struct task_struct *)ctx[1];
  pid_t pid, parent_tgid;
  int ret = bpf_core_read(&pid, sizeof(pid), &child->pid);
  if (ret < 0) {
    debug("Failed to read child pid of fork: %d", ret);
    return -EFAULT;
  }
  ret = bpf_core_read(&parent_tgid, sizeof(pid), &parent->tgid);
  if (ret < 0) {
    debug("Failed to read parent tgid of fork: %d", ret);
    return -EFAULT;
  }
  if (should_trace(parent_tgid)) {
    add_tgid_to_closure(pid);
  }
  return 0;
}

SEC("tracepoint/syscalls/sys_enter_execve")
int tp_sys_enter_execve(struct sys_enter_execve_args *ctx) {
  struct task_struct *task;
  struct exec_event *event;
  struct sys_enter_exec_args common_ctx = {.syscall_nr = ctx->__syscall_nr,
                                           .argv = ctx->argv,
                                           .envp = ctx->envp,
                                           .base_filename = ctx->filename};
  trace_exec_common(&common_ctx);
  return 0;
}

SEC("tracepoint/syscalls/sys_exit_execve")
int tp_sys_exit_execve(struct sys_exit_exec_args *ctx) {
  pid_t pid, tgid;
  u64 tmp = bpf_get_current_pid_tgid();
  pid = (pid_t)tmp;
  tgid = tmp >> 32;
  // debug("sysexit: pid=%d, tgid=%d, ret=%d", pid, tgid, ctx->ret);
  struct exec_event *event;
  event = bpf_map_lookup_elem(&execs, &pid);
  if (event == NULL) {
    debug("Failed to lookup exec_event on sysexit");
    drop_counter += 1;
    return 0;
  }
  // Use the old tgid. If the exec is successful, tgid is already set to pid.
  if (!should_trace(event->tgid)) {
    if (0 != bpf_map_delete_elem(&execs, &pid)) {
      debug("Failed to del element from execs map");
    }
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
  struct file **fd_array;
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
  if (offset >= BITS_PER_LONG)
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
  struct file **fd_array = ctx->fd_array;
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
  struct fdset_word_reader_context subctx = {
      .fdset = fdset,
      .event = event,
      .fd_array = fd_array,
      .next_bit = BITS_PER_LONG,
      .word_index = index,
  };
  subctx.next_bit = find_next_bit(fdset, 0);
  bpf_loop(BITS_PER_LONG, read_fdset_word, &subctx, 0);
  return 0;
}

static int read_fdset_word(u32 index, struct fdset_word_reader_context *ctx) {
  if (ctx == NULL)
    return 1;
  if (ctx->next_bit == BITS_PER_LONG)
    return 1;
  unsigned int fdnum = ctx->next_bit + BITS_PER_LONG * ctx->word_index;
  _read_fd(fdnum, ctx->fd_array, ctx->event);
  ctx->next_bit = find_next_bit(ctx->fdset, ctx->next_bit + 1);
  return 0;
}

// Gather information about a single fd and send it back to userspace
static int _read_fd(unsigned int fd_num, struct file **fd_array,
                    struct exec_event *event) {
  if (event == NULL)
    return 1;
  event->fd_count++;
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
  entry->fd = fd_num;
  // read f_path
  struct file *file;
  int ret = bpf_core_read(&file, sizeof(void *), &fd_array[fd_num]);
  if (ret < 0) {
    debug("failed to read file struct: %d", ret);
    goto ptr_err;
  }
  struct path path;
  ret = bpf_core_read(&path, sizeof(path), &file->f_path);
  // read name
  entry->path_id = event->path_count++;
  ret = read_send_path(&path, &entry->header, entry->path_id);
  if (ret < 0) {
    event->header.flags |= PATH_READ_ERR;
  }
  entry->flags = 0;
  ret = bpf_core_read(&entry->flags, sizeof(entry->flags), &file->f_flags);
  if (ret < 0) {
    debug("failed to read file->f_flags: %d", ret);
    entry->flags |= FLAGS_READ_FAILURE;
  }
  debug("open fd: %u -> %u with flags %u", fd_num, entry->path_id,
        entry->flags);
  bpf_ringbuf_output(&events, entry, sizeof(struct fd_event), 0);
  return 0;
ptr_err:
  entry->header.flags |= PTR_READ_FAILURE;
  entry->path_id = -1;
  bpf_ringbuf_output(&events, entry, sizeof(struct fd_event), 0);
  return 1;
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
  entry->header.id = index + ctx->index * event->count[0];
  s64 bytes_read =
      bpf_probe_read_user_str(entry->data, sizeof(entry->data), argp);
  if (bytes_read < 0) {
    debug("failed to read arg %d(addr:%x) from userspace", index, argp);
    entry->header.flags |= STR_READ_FAILURE;
    // Replace such args with '\0'
    entry->data[0] = '\0';
    bytes_read = 1;
  } else if (bytes_read == 0) {
    debug("read arg %d(addr:%x) = %ld", index, argp, bytes_read);
    entry->data[0] = '\0';
    bytes_read = 1;
  } else if (bytes_read == sizeof(entry->data)) {
    entry->header.flags |= POSSIBLE_TRUNCATION;
  }
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

static int add_tgid_to_closure(pid_t tgid) {
  char dummy = 0;
  int ret = bpf_map_update_elem(&tgid_closure, &tgid, &dummy, 0);
  if (ret < 0) {
    // Failed to insert to tgid closure. This shouldn't happen on a standard
    // kernel.
    debug("Failed to insert %d into tgid_closure, this shouldn't happen on a "
          "standard kernel: %d",
          tgid, ret);
    // TODO: set a flag to notify user space
    return ret;
  }
  return 0;
}

struct path_segment_ctx {
  struct dentry *dentry;
  struct dentry *mnt_root;
  struct dentry *root;
  struct path_event *path_event;
  const struct event_header *base_header;
  u32 base_index;
};

static int read_send_dentry_segment(u32 index, struct path_segment_ctx *ctx);

// Read all dentry segments upto the mount point and send them to userspace.
// Returns the number of segments iterated on success, -1 on failure
static inline int read_send_dentry_segments_recursive(
    struct path_segment_ctx *ctx, struct path_event *path_event, u32 path_id) {

  // while dentry->d_parent != dentry, read dentry->d_name.name and send it back
  // to userspace
  int ret = bpf_loop(PATH_DEPTH_MAX, read_send_dentry_segment, ctx, 0);
  if (ret < 0) {
    debug("Failed to iter over dentry segments: %d!", ret);
    path_event->header.flags |= LOOP_FAIL;
    return -1;
  }

  return ret;
}

// bpf_loop helper:
static int read_send_dentry_segment(u32 index, struct path_segment_ctx *ctx) {
  int ret = 1; // break
  if (ctx == NULL || ctx->base_header == NULL)
    return ret;
  // Read this segment
  long key = 0;
  struct path_segment_event *event =
      bpf_map_lookup_elem(&path_segment_cache, &key);
  if (event == NULL)
    return ret;
  event->header = (struct event_header){
      .id = ctx->path_event->header.id,
      .type = PATH_SEGMENT_EVENT,
      .eid = ctx->base_header->eid,
      .pid = ctx->base_header->pid,
      .flags = 0,
  };
  event->index = index + ctx->base_index;

  unsigned char *name;
  struct dentry *dentry = ctx->dentry;
  ret = bpf_core_read(&name, sizeof(void *), &dentry->d_name.name);
  if (ret < 0) {
    debug("failed to read dentry->d_name.name: %d, dentry = %p", ret, dentry);
    event->header.flags |= PTR_READ_FAILURE;
    event->segment[0] = '\0';
    goto send;
  }
  ret = bpf_probe_read_kernel_str(&event->segment, PATH_SEGMENT_MAX, name);
  if (ret < 0) {
    debug("failed to read name string: %d", ret);
    event->header.flags |= STR_READ_FAILURE;
    event->segment[0] = '\0';
  }
send:;
  // Send this segment to user space
  ret = bpf_ringbuf_output(&events, event, sizeof(*event), 0);
  if (ret < 0) {
    ctx->path_event->header.flags |= OUTPUT_FAILURE;
  }
  // Check if we reached mount point
  struct dentry *parent;
  ret = bpf_core_read(&parent, sizeof(void *), &dentry->d_parent);
  if (ret < 0) {
    debug("failed to read dentry->d_parent: %d", ret);
    ctx->path_event->header.flags |= BAIL_OUT;
    ctx->dentry = NULL;
    // break
    return 1;
  }
  debug("got segment: index:%d, %s, dentry = %p, mnt_root = %p, parent = %p",
        event->index, event->segment, ctx->dentry, ctx->mnt_root, parent);
  if (parent == ctx->dentry || parent == ctx->mnt_root || parent == ctx->root) {
    debug("skipping: parent=%p, root = %p, mnt_root = %p", parent, ctx->root,
          ctx->mnt_root);
    // break
    return 1;
  }
  ctx->dentry = parent;
  // continue
  return 0;
}

struct mount_ctx {
  struct mount *mnt;
  struct path_event *path_event;
  int base_index;
  u32 path_id;
  struct path_segment_ctx *segment_ctx;
};

// root: current->fs->root
// bpf_loop helper:
static int read_send_mount_segments(u32 index, struct mount_ctx *ctx) {
  int ret = 1; // break
  if (ctx == NULL || ctx->path_event == NULL)
    return ret;
  // Read the mountpoint dentry
  struct dentry *mnt_mountpoint, *mnt_root;
  struct mount *parent, *mnt = ctx->mnt;
  struct mountpoint *mountpoint;
  // struct vfsmount *vfsmnt;
  ret = bpf_core_read(&mnt_mountpoint, sizeof(void *), &mnt->mnt_mountpoint);
  if (ret < 0) {
    debug("failed to read mnt->mnt_mountpoint: %d", ret);
    goto err_out;
  }
  ret = bpf_core_read(&parent, sizeof(void *), &mnt->mnt_parent);
  if (ret < 0) {
    debug("failed to read mnt->mnt_parent: %d", ret);
    goto err_out;
  }
  ret = bpf_core_read(&mnt_root, sizeof(void *), &parent->mnt.mnt_root);
  if (ret < 0) {
    debug("failed to read mnt->mnt.mnt_root: %d", ret);
    goto err_out;
  }
  int mnt_id;
  ret = bpf_core_read(&mnt_id, sizeof(int), &mnt->mnt_id);
  if (ret < 0) {
    debug("failed to read mnt->mnt_id: %d", ret);
    goto err_out;
  }
  // Break if we reached top mount
  if (parent == mnt) {
    // break
    debug("should break");
    return 1;
  }
  debug("iter mount %p %d", mnt, mnt_id);
  *ctx->segment_ctx = (struct path_segment_ctx){
      .path_event = ctx->path_event,
      .base_header = &ctx->path_event->header,
      .dentry = mnt_mountpoint,
      .mnt_root = mnt_root,
      .root = ctx->segment_ctx->root,
      .base_index = ctx->base_index,
  };
  // Read the segments and send them to userspace
  int iter_nr = read_send_dentry_segments_recursive(
      ctx->segment_ctx, ctx->path_event, ctx->path_id);
  if (iter_nr < 0) {
    goto err_out;
  }
  ctx->base_index += iter_nr;
  ctx->mnt = parent;
  return 0;
err_out:
  // If we failed to read the segments of this mount, send a placeholder to
  // userspace
  // TODO
  debug("Failed to send mount %p", mnt);
  // continue
  return 0;
}

// Read all dentry path segments up to mnt_root,
// and then read all ancestor mount entries to reconstruct
// an absolute path.
//
// Arguments:
//   path: a pointer to a path struct, this is not a kernel pointer
static int read_send_path(const struct path *path,
                          const struct event_header *base_header, s32 path_id) {
  int ret = -1;
  // Initialize
  int index = 0;
  struct path_event *event = bpf_map_lookup_elem(&path_event_cache, &index);
  if (event == NULL) {
    debug("This should not happen!");
    return 1;
  }

  *event = (struct path_event){
      .header = {.eid = base_header->eid,
                 .pid = base_header->pid,
                 .flags = 0,
                 .id = path_id,
                 .type = PATH_EVENT},
      .segment_count = 0,
  };
  event->header.flags = 0;
  // Get root dentry
  struct task_struct *current = (void *)bpf_get_current_task();
  struct dentry *root;
  struct fs_struct *fs;
  ret = bpf_core_read(&fs, sizeof(void *), &current->fs);
  if (ret < 0) {
    debug("failed to read current->fs: %d", ret);
    goto ptr_err;
  }
  ret = bpf_core_read(&root, sizeof(void *), &fs->root.dentry);
  if (ret < 0) {
    debug("failed to read fs->root.dentry: %d", ret);
    goto ptr_err;
  }
  // Get vfsmount and mnt_root
  struct vfsmount *vfsmnt = path->mnt;
  struct dentry *mnt_root, *dentry = path->dentry;
  ret = bpf_core_read(&mnt_root, sizeof(void *), &vfsmnt->mnt_root);
  if (ret < 0) {
    debug("failed to read vfsmnt->mnt_root: %d", ret);
    goto ptr_err;
  }
  struct path_segment_ctx segment_ctx = {
      .path_event = event,
      .base_header = &event->header,
      .dentry = dentry,
      .mnt_root = mnt_root,
      .root = root,
      .base_index = 0,
  };
  // Send the dentry segments to userspace
  index = read_send_dentry_segments_recursive(&segment_ctx, event, path_id);
  if (index < 0) {
    goto loop_err;
  }
  // Iterate over all ancestor mounts and send segments to userspace
  struct mount *mnt = container_of(vfsmnt, struct mount, mnt);
  struct mount_ctx ctx = {
      .base_index = index,
      .mnt = mnt,
      .path_event = event,
      .path_id = path_id,
      // Reuse the above segment_ctx to save stack space
      .segment_ctx = &segment_ctx,
  };
  ret = bpf_loop(PATH_DEPTH_MAX, read_send_mount_segments, &ctx, 0);
  if (ret < 0) {
    goto loop_err;
  }
  // Send path event to userspace
  event->segment_count = ctx.base_index;
  ret = bpf_ringbuf_output(&events, event, sizeof(*event), 0);
  if (ret < 0) {
    debug("Failed to output path_event to ringbuf");
    return -1;
  }
ptr_err:
  event->header.flags |= PTR_READ_FAILURE;
  goto err_out;
loop_err:
  event->header.flags |= LOOP_FAIL;
  goto err_out;
err_out:
  event->segment_count = 0;
  ret = bpf_ringbuf_output(&events, event, sizeof(*event), 0);
  if (ret < 0) {
    debug("Failed to output path_event to ringbuf");
    return -1;
  }
  return -1;
}