# Backend Differences

The ptrace and eBPF backends produce the same core event types, but they do not
observe the kernel from the same place. Code shared between them must account
for differences in scope, timing, data quality, and process control.

| Property | ptrace | eBPF |
| --- | --- | --- |
| Invocation | `tracexec <frontend>` | `tracexec ebpf <frontend>` |
| Scope | One launched command tree | One launched command tree or the whole system |
| Privilege | Usually the tracee's user | Root or suitable capabilities |
| setuid/setgid exec | Restricted by ptrace rules | Observable |
| User-memory inspection | Tracee is stopped; reads are generally reliable | Reads can be partial or fail without faulting pages in when the sleepable program types are not used |
| Process control | Can stop, resume, and detach tracees | Observation only |
| Debugger coexistence | A tracee cannot have another ptrace tracer | Can observe a process controlled by GDB/strace |
| Main optimization | seccomp-BPF limits ptrace stops to relevant syscalls | - |


## Scope and lifecycle

The ptrace backend starts a root tracee and follows forks, clones, and execs in
that tree. Its completion condition is tied to that root command. The eBPF
backend can do the same scoped filtering, but with no command it observes
system-wide activity until interrupted.

## Inspection timing

ptrace handles a syscall while the tracee is stopped. It can inspect registers
and `/proc/<pid>` state at a defined syscall boundary. Even then, reads may
fail because a process exited or procfs denied access.

eBPF programs copy data while running in kernel context. A userspace address
may not be resident, and when the sleepable variant of the programs are not used, BPF helpers cannot resolve that by taking an ordinary
page fault. Fields therefore use `OutputMsg`, `Result`, or another fallible
wrapper. Preserve partial values instead of converting them into an empty
string or empty collection.

## Process and thread identity

Linux can execute a program from a non-leader thread. Exec collapses the thread
group and may change the visible task ID. `ExecEvent` keeps both `exec_pid` (the
task that entered exec) and `pid` (the process identity presented after the
event). Backends must populate both according to the shared event contract.
Consumers should not silently substitute one for the other.

## Control path

Only the ptrace backend has a reverse control channel. `RunningTracer` sends
`PendingRequest` values to resume or detach a breakpoint hit, suspend the
seccomp optimization, or terminate the tracer. The TUI's breakpoint and
debugger features depend on that channel and must stay hidden in eBPF mode.

The seccomp optimization also affects detach behavior. A detached process
retains the filter; without the tracer, a later exec can no longer be serviced.
The UI warns users to start with `--seccomp-bpf=off` when a detached process
needs to exec again.

## Adding shared behavior

When adding a field or event:

1. define its meaning in backend-neutral terms;
2. implement and test collection in both backends, including failure cases;
3. decide whether an absent value, a partial value, and an inspection error
   need distinct representations;
4. check log, TUI, JSON, JSON-stream, and Perfetto consumers;
5. test scoped eBPF and system-wide eBPF separately when lifecycle matters.

If one backend cannot provide a field honestly, return an explicit unsupported
or failed state. A plausible fabricated value is harder to debug than a marked
gap.
