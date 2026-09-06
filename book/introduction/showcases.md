# Showcases

The following examples illustrate how tracexec can be used to inspect builds,
trace command execution, and launch a debugger.

## Perfetto Trace Export

tracexec supports exporting exec traces to perfetto trace format,
which could be viewed in the [Perfetto UI](https://ui.perfetto.dev/).
The trace follows a tree format in the UI, where processes resulting from successful execs are represented as slices and exec failures are represented as instant events.

The following video shows analyzing the build process of tracexec with itself:

<video
  src="https://github.com/user-attachments/assets/fc825d55-eb2f-43cd-b454-a11b5f9b44bc"
  controls preload="none" loading="lazy"
  poster="../assets/perfetto-build-cover.jpg"
  width="100%">
  <a href="https://github.com/user-attachments/assets/fc825d55-eb2f-43cd-b454-a11b5f9b44bc">Watch the Perfetto build analysis demo.</a>
</video>

The shape of the traces in the Perfetto UI could give you a rough idea of how
parallel the build is at process-level. The trace tree and details of slices
enable identification of bottlenecks, troubleshooting, and a deep understanding
of how the build works.

Start collecting a perfetto trace with the following command:

```bash
tracexec collect --format=perfetto -o out.pftrace -- cmd
```

See [Perfetto Trace Export](../features/collect/perfetto.md) for instructions on
collecting and interpreting a trace.

## TUI mode with pseudo terminal

TUI mode allocates a pseudo terminal by default, allowing you to view the details of exec events and interact
with the processes within the pseudo terminal. Use `--no-tty` when a pseudo terminal is not wanted; the tracee's
stdin, stdout, and stderr will be redirected to `/dev/null`.

![TUI demo](https://github.com/kxxt/tracexec/blob/v0.15.1/screenshots/tui-demo.gif?raw=true)

## Tracing setuid binaries

With root privileges, you can also trace setuid binaries and see how they work.
But do note that this is not compatible with seccomp-bpf optimization so it is much less performant.
You can use eBPF mode which is more performant in such scenarios.

```bash
sudo tracexec --user $(whoami) tui -- sudo ls
```

![Tracing sudo ls](https://github.com/kxxt/tracexec/blob/v0.15.1/screenshots/tracing-sudo.png?raw=true)

Nested setuid binary tracing is also possible: A real world use case is to trace `extra-x86_64-build`(Arch Linux's build tool that requires sudo):

![Tracing extra-x86_64-build](https://github.com/kxxt/tracexec/blob/v0.15.1/screenshots/tracing-nested-setuid.gif?raw=true)

In this real world example, we can easily see that `_FORTIFY_SOURCE` is redefined from `2` to `3`, which led to a compiler error.

## Use tracexec as a debugger launcher

tracexec can also be used as a debugger launcher to make debugging programs easier. For example, it's not trivial or convenient
to debug a program executed by a shell/python script(which can use pipes as stdio for the program). The following video shows how to
use tracexec to launch GDB to attach to two simple programs piped together by a shell script.

<video
  src="https://github.com/kxxt/tracexec/assets/18085551/72c755a5-0f2f-4bf9-beb9-98c8d6b5e5fd"
  controls preload="none" loading="lazy"
  poster="../assets/gdb-launcher-cover.jpg"
  width="100%">
  <a href="https://github.com/kxxt/tracexec/assets/18085551/72c755a5-0f2f-4bf9-beb9-98c8d6b5e5fd">Watch the debugger launcher demo.</a>
</video>

See the [debugger-launcher tutorial](../tutorials/debugger-launcher.md) for the complete example.

## eBPF mode

Please check [platform support status](../support.md#linux-kernel-support-status) before using the eBPF backend.

The following examples show how to use eBPF in TUI mode.
The `ebpf` command also supports regular `log` and `collect` subcommands.

### System-wide Exec Tracing

System-wide tracing has no command to attach to a pseudo terminal, so it runs without one automatically:

```bash
sudo -E tracexec ebpf tui
```

<video
  src="https://github.com/user-attachments/assets/12cec4ef-8884-4580-a93a-c9144ec7102b"
  controls preload="none" loading="lazy"
  poster="../assets/ebpf-system-wide-cover.jpg"
  width="100%">
  <a href="https://github.com/user-attachments/assets/12cec4ef-8884-4580-a93a-c9144ec7102b">Watch the system-wide eBPF tracing demo.</a>
</video>

### Follow Fork mode with eBPF

```bash
sudo -E tracexec --user $(whoami) ebpf tui -- bash
```

<video
  src="https://github.com/user-attachments/assets/997e1992-df85-4d45-ae68-faf693c6b99b"
  controls preload="none" loading="lazy"
  poster="../assets/ebpf-follow-forks-cover.jpg"
  width="100%">
  <a href="https://github.com/user-attachments/assets/997e1992-df85-4d45-ae68-faf693c6b99b">Watch the scoped eBPF tracing demo.</a>
</video>

## Log mode

In log mode, by default, `tracexec` will print filename, argv and the diff of the environment variables and file descriptors.

example: `tracexec log -- bash` (In an interactive bash shell)

[![asciicast](https://asciinema.org/a/sNptWG6De3V5xwUvXJAxWlO3i.svg)](https://asciinema.org/a/sNptWG6De3V5xwUvXJAxWlO3i)

## Reconstruct the command line with `--show-cmdline`

```bash
$ tracexec log --show-cmdline -- <command>
# example:
$ tracexec log --show-cmdline -- firefox
```

[![asciicast](https://asciinema.org/a/AWTG4iHaFPMcEGCVtqAl44YFW.svg)](https://asciinema.org/a/AWTG4iHaFPMcEGCVtqAl44YFW)

## Try to reproduce stdio in the reconstructed command line

`--stdio-in-cmdline` and `--fd-in-cmdline` can be used to reproduce(hopefully) the stdio used by a process.

But do note that the result might be inaccurate when pipes, sockets, etc are involved.

```bash
tracexec log --show-cmdline --stdio-in-cmdline -- bash
```

[![asciicast](https://asciinema.org/a/NkBTaoNHS7P7bolO0hNuRwGlQ.svg)](https://asciinema.org/a/NkBTaoNHS7P7bolO0hNuRwGlQ)

## Show the interpreter indicated by shebang with `--show-interpreter`

And show the cwd with `--show-cwd`.

```bash
$ tracexec log --show-interpreter --show-cwd -- <command>
# example: Running Arch Linux makepkg
$ tracexec log --show-interpreter --show-cwd -- makepkg -f
```

[![asciicast](https://asciinema.org/a/7jDtrlNRx5XUnDXeDBsMRj09p.svg)](https://asciinema.org/a/7jDtrlNRx5XUnDXeDBsMRj09p)
