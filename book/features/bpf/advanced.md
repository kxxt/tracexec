# Advanced Parameters for eBPF Backend

The parameters listed here are not considered a stable interface.
**They may be MODIFIED or completely REMOVED and it would not be considered as a breaking change.**

**You should only use parameters from this page if you understand it.**

## `TRACEXEC_NO_SLEEP` env var

By default, tracexec automatically detects whether the kernel supports sleepable `fentry`
eBPF programs. If this environment variable is set to a non-empty value, tracexec will use
non-sleepable eBPF programs for `fentry` of exec syscalls.

When `fentry` is disabled and `kprobe` is used, this setting has no effect.

## `TRACEXEC_USE_FENTRY/KPROBE` env vars

By default, tracexec automatically detects whether the kernel supports `CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS`
to decide whether to use `fentry/fexit` or `kprobe/kretprobe`. These two environment variables could override it.

- When `TRACEXEC_USE_FENTRY` is set to a non-empty value, tracexec will use `fentry/fexit` eBPF programs.
- When `TRACEXEC_USE_KPROBE` is set to a non-empty value, tracexec will use `kprobe/kretprobe` eBPF programs.
- Setting both variables simultaneously is not supported and may produce unpredictable results.