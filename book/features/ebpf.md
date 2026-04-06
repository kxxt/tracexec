# eBPF Backend

eBPF is an advanced backend and currently considered [experimental](https://github.com/kxxt/tracexec/issues/49).

To use this backend, run tracexec with `ebpf` as subcommand and the desired frontend as sub-subcommand
(`tracexec ebpf log` or `tracexec ebpf tui` for example).

## A Brief Introduction to eBPF

eBPF is a revolutionary technology for running sandboxed and verified programs directly in the Linux kernel.
The in-kernel BPF verifier verifies the program before loading it into the kernel to ensure its safety.
For tracing exec, eBPF enables us to attach tracing eBPF programs to kernel functions that handle `execve` and
`execveat` syscalls and other scheduler tracepoints like `sched_process_fork` that fires when a process creates
a new thread or a new process.

## Strengths

- System-wide tracing makes the eBPF backend well-suited for system observability.
- Scoped tracing is also implemented.
- Does not use `ptrace(2)` so
  - Tracing setuid/setgid binaries is supported.
  - You could combine it with other tools that use `ptrace(2)`, e.g. gdb.

## Weaknesses

- Requires root privilege. (or a bunch of capabilities like `CAP_SYS_ADMIN` and `CAP_BPF`)
- Sometimes reading userspace memory will fail due to page fault, causing the trace to miss some information.
  - See also <https://mozillazg.com/2024/03/ebpf-tracepoint-syscalls-sys-enter-execve-can-not-get-filename-argv-values-case-en.html>
  - This could be solved once tracexec is migrated to use sleepable eBPF programs.
- Requires loading eBPF code into Linux kernel, which might be forbidden in kernel lockdown mode.
- Sometimes there are kernel eBPF bugs that could reject the eBPF program.
