#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>

char LICENSE[] SEC("license") = "GPL";

SEC("tracepoint/syscalls/sys_enter_execve")

int tp_sys_enter_execve(struct sys_enter_execve_args *ctx) {
    return 0;
}