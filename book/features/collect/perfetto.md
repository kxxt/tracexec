# Perfetto Trace Export

The Perfetto exporter turns an exec trace into a timeline that you can explore in the
[Perfetto UI](https://ui.perfetto.dev/).
It is useful when you want to see how a build or shell script runs: which programs it starts,
how long they last and which ones run in parallel.

## Collecting a Trace

Use `--format perfetto` and give the output file a name:

```bash
tracexec collect --format perfetto --output trace.pftrace -- bash -c 'sleep 1 & sleep 2 & wait'
```

This example starts two `sleep` processes in parallel and waits for both to finish.
The trace should show a shell lasting about two seconds, with two child slices lasting
about one and two seconds.

Replace the command after `--` with the program you want to trace.
For example, to collect a parallel build:

```bash
tracexec collect --format perfetto -o build.pftrace -- make -j4
```

`-o` is the short form of `--output`.
The trace is a binary file, so use an output file to keep it separate from the traced
program's terminal output. Wait for collection to finish before opening it.

The exporter also works with the [eBPF backend](../ebpf.md):

```bash
tracexec --elevate ebpf collect --format perfetto -o build.pftrace -- make -j4
```

> [!WARNING]
> The trace file may contain sensitive credentials that are passed in commandline arguments
> or environment variables. Sharing the trace file may leak such credentials. 


## Opening the Trace

Open [ui.perfetto.dev](https://ui.perfetto.dev/) and choose `Open trace file`,
or drag your `.pftrace` file into the page.

- Use <kbd>W</kbd>/<kbd>S</kbd> to zoom in/out and <kbd>A</kbd>/<kbd>D</kbd> to pan left/right.
- Click a slice to inspect it in the `Current Selection` panel.
- Press <kbd>F</kbd> to center the selected slice, then <kbd>F</kbd> again to fit it in the view.

See Perfetto's [UI guide](https://perfetto.dev/docs/visualization/perfetto-ui)
for more navigation shortcuts.

## Interpreting the Trace

Successful execs appear as **slices**, the horizontal bars in the timeline.
The duration of slices represents wall time instead of CPU time.
A slice starts at an exec event and ends when that program exits, is replaced by another
successful exec in the same process, or is detached from tracexec.
Its name comes from `argv[0]`, falling back to the executable filename when the arguments
are unavailable.

The tracks form a tree. When a process spawns a child that executes a program, the child's
slice appears on a track below its parent's track.
When the same process executes another program, the old slice ends and the new one starts
on the same track.

tracexec reuses available child tracks to keep the view compact,
so a row can contain different processes at different times.
Check the selected slice's `pid` argument when you need to identify a process.

Failed exec attempts appear as **instant events** instead of slices.
For example, a program searching `PATH` may try several filenames before finding one that exists.
Select an instant event and check `filename` and `syscall_ret` to see what failed.
If you only want successful execs, add `--successful-only` when collecting the trace:

```bash
tracexec collect --format perfetto --successful-only -o build.pftrace -- make -j4
```

## Inspecting an Event

Select a slice or instant event and expand its arguments in `Current Selection`.
tracexec attaches the following information, when available:

| Argument | What it contains |
| --- | --- |
| `argv` | The argument list, including `argv[0]`. |
| `filename` | The executable filename. |
| `cmdline` | A reconstructed Bash command line, including environment and working directory changes. |
| `cwd` | The working directory at exec. |
| `pid` | The process ID. |
| `syscall_ret` | The exec syscall result: zero for success, or a negative error number. |
| `env` | The full environment passed to exec. |
| `fd` | File descriptors, with their paths, flags, positions, mount information and other collected details. |
| `interpreter` | Interpreter information. |
| `cred` | User IDs, group IDs and supplementary groups. |
| `cgroup` | The cgroup v2 path, if collected, or a description of why it is unavailable. |

To include cgroup information, add `--collect-cgroup` when recording.

Completed slices also carry `end_reason`. For example, `exec` means the process replaced
itself with another process, `exited` means it exited with an exit code, and `signaled` means it was killed
by a signal. `exit_code` or `exit_signal` provides the corresponding result when available.
This lets you distinguish a successful exec followed by a program failure from an exec
call that failed to start the program at all.

## Example: Building tracexec

The following video uses tracexec to analyze its own build.
After the build finishes, it shows the overall timeline and looks more closely at individual
programs and their arguments.

<video
  src="https://github.com/user-attachments/assets/fc825d55-eb2f-43cd-b454-a11b5f9b44bc"
  controls preload="none" loading="lazy"
  poster="../../assets/perfetto-build-cover.jpg"
  width="100%">
  <a href="https://github.com/user-attachments/assets/fc825d55-eb2f-43cd-b454-a11b5f9b44bc">Watch the Perfetto build analysis demo.</a>
</video>

To trace a Rust build in the same way, run this in the project's directory:

```bash
tracexec collect --format perfetto -o build.pftrace -- cargo build
```
