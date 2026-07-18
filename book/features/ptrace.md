# Ptrace Backend

[`ptrace(2)`] is the default backend for tracexec.
To use this backend, simply run `tracexec` with the desired frontend subcommand (`log`, `tui` and `collect`).

## A Simple Introduction to Ptrace

[`ptrace(2)`] is the interface designed for implementing a debugger.
It allows a tracer process to attach to a tracee process and do basically almost anything to it,
such as reading/writing its registers and memories, intercepting its syscall and single-step debugging.
A single tracer could trace multiple tracees concurrently but a single tracee could only be traced by
one tracer at any given time.

[strace](https://strace.io/) is a generic syscall tracing tool built upon [`ptrace(2)`],
while tracexec is a specialized tool for tracing exec syscall and related contexts.

But wait, isn't ptrace slow since it is a syscall interface meant for debuggers?
Would it slow down workloads significantly? It is indeed slow when used in default
configuration because we need to stop/resume the program at every syscall it makes.
But when combined with [`seccomp(2)`], the overhead could actually be reduced to minimal.
[`seccomp(2)`] implements a fast syscall filtering interface with classic BPF, by combining
[`ptrace(2)`] with a [`seccomp(2)`] filter that only notifies us when the exec syscalls happen,
we avoid incurring overhead on other syscalls the tracee makes.
In case you want to learn more about this optimization, read the
[well-written blog post from strace developer](https://pchaigno.github.io/strace/2019/10/02/introducing-strace-seccomp-bpf.html).

## Subprocess Job Control

The ptrace backend can control subprocess parallelism without cooperating with
the build system. Pass `--job-control auto` to a ptrace frontend and allow the
build system to create jobs without its own limit. For example:

```console
tracexec log --job-control auto -- make -j
```

Automatic job control has no fixed job-count or core-count limit. On kernels
with Pressure Stall Information (PSI), it measures CPU contention and full
memory stalls instead of treating fully utilized CPUs or a fixed percentage of
reclaimable memory as unavailable. CPU utilization and available memory are
used as fallbacks when PSI is unavailable.

Additional subprocesses are paused at successful exec syscall-exit stops only
while resources are constrained. A completed process tree is replaced
immediately; when pressure drops, additional paused jobs are resumed gradually
in FIFO order to avoid a thundering herd. Descendants inherit their parent's
admission, so a job can run and wait for helper processes such as a compiler
waiting for its assembler without deadlocking. The traced root command is not
counted as a job, and one process tree is always allowed to run to guarantee
forward progress.

By default, job control uses blocking `waitpid` so ptrace events are handled as
soon as they arrive. A lightweight timer thread interrupts the blocked tracer
only for periodic resource reevaluation; it does not poll for ptrace events.
The timer thread remains parked while no subprocesses are waiting. An
explicitly configured positive `--polling-interval` still selects polling mode.

## Strengths

- Works out of the box.
- Low overhead when combined with [`seccomp(2)`]. (default in tracexec)
- The minimum required Linux kernel version is 5.3.
- [Makes it possible to conveniently attach a debugger to a newly spawned process](./tui/debugger.md).

## Weaknesses

- Cannot perform system-wide tracing.
- Does not work with setuid/setgid binaries out of the box.
- Significant overhead when [`seccomp(2)`] optimization is not used.
- ~~[`ptrace(2)`] is a very complex interface abusing `waitpid(2)` and signals.~~

[`ptrace(2)`]: https://man7.org/linux/man-pages/man2/ptrace.2.html
[`seccomp(2)`]: https://man7.org/linux/man-pages/man2/seccomp.2.html
