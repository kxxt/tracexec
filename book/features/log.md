# Log Frontend

Logging is the most simple frontend for tracexec. This frontend simply logs exec events to
the terminal or a file.

To use the log frontend,
- run `tracexec log` to use [ptrace backend](./ptrace.md),
- or run `tracexec ebpf log` to use [eBPF backend](./ebpf.md).

By default, the log frontend shows the `filename`, `argv` and diff of environment variables, along with
basic information like PID, `comm` before exec and syscall result.

## Log Format

This section introduces the UI elements in the log lines.
For example, let's simply run `ls` with a new environment variable:

### Basic Elements

```bash
tracexec log -- env A=B ls
```

<!-- asciinema record --window-size 80x24 --overwrite --command "tracexec log -- env A=B ls" ~/repos/tracexec/book/casts/log-ls.cast -->

{{ #asciinema ../casts/log-ls.cast opts=casts/autoplay.json }}

As shown in the output,
1. The tracer forks and executes `/usr/bin/env`, with our supplied command line arguments.
    - The `exec` syscall is successful so the pid at the start of the line is in green color.
    - The [`comm`] of the original process is `tracer` before exec.
2. `/usr/bin/env` tried to execute `ls` program from paths listed in the `PATH` environment variable.
    - The first few attempts failed because `ls` program is not found in the attempted directory. The return value `-2` is displayed at the end of the line with a user-friendly error message. The pid at the start of the line is displayed in yellow color, meaning a usually non-fatal error occurred.
    - Since we specified `A=B` in the commandline, `env` adds this environment variable when executing `ls`. `tracexec` shows this added environment variable in green color with a plus sign, indicating it is a new env var.
3. At last, `/usr/bin/env` successfully executed `/usr/bin/ls` and the output from `ls` is shown.

[`comm`]: https://man7.org/linux/man-pages/man5/proc_pid_comm.5.html

### Timestamp

By default, tracexec does not show the timestamp for the events.
Use `--timestamp` to enable it.

For example:

```bash
tracexec log --timestamp -- env ls /
```

<!-- asciinema record --window-size 80x20 --overwrite --command "tracexec log --timestamp -- env ls /" ~/repos/tracexec/book/casts/log-timestamp.cast -->

{{ #asciinema ../casts/log-timestamp.cast opts=casts/autoplay.json }}

It is possible to customize the format of the timestamp with `--inline-timestamp-format <INLINE_TIMESTAMP_FORMAT>`.
See <https://docs.rs/chrono/latest/chrono/format/strftime/index.html> for available variables that can be used in the format string.

### Verbosity of Environment Variables

By default, tracexec only outputs the diff of environment variables against the initial environment.

To increase the verbosity to show all environment variables, use `--show-env`.
e.g. 

```bash
tracexec log --show-env -- ls
```

To hide the environment variables entirely, use `--no-show-env`.

### Reconstruct the Shell Commandline

A handy feature of tracexec is to show the equivalent shell commandlines of the exec events.
This feature makes it easy to reproduce the exec events in a shell.

For example:

```bash
tracexec log --show-cmdline -- env -u LANG A=B ls
```

<!-- asciinema record --window-size 80x26 --overwrite --command "tracexec log --show-cmdline -- env -u LANG A=B ls" ~/repos/tracexec/book/casts/log-ls-cmdline.cast -->

{{ #asciinema ../casts/log-ls-cmdline.cast opts=casts/autoplay.json }}

In this example, we use `-u LANG` to remove the `LANG` environment variable and `A=B` to set `A` env to `B` for the ls command.
`tracexec` shows the equivalent shell commandline of the successful execution as `env -a ls -u LANG A=B /usr/bin/ls`, which could be directly
copy-and-pasted into a bash shell.

### File Descriptor Tracking

File descriptors are inherited during `exec` unless the file descriptor is marked with `O_CLOEXEC`.
Bugs or even security vulnerabilities may occur if a program forgets
to close file descriptors after `fork` and before `exec`.
However, sometimes it is normal to keep some file descriptors open in order to pass them to the child process.

`tracexec` tracks the file descriptors used during exec and shows a diff of file descriptors by default.

In the following example, we run `cat` with stdin closed and stdout redirected to `/dev/null` and a new file descriptor of `/dev/random`.

```bash
tracexec log -- bash -c "cat <&- > /dev/null 4</dev/random"
```

<!-- asciinema record --window-size 80x8 --overwrite --command "tracexec log -- bash -c \"cat <&- > /dev/null 4</dev/random\"" ~/repos/tracexec/book/casts/log-fd.cast -->

{{ #asciinema ../casts/log-fd.cast opts=casts/autoplay.json }}

tracexec shows the file descriptor diff with three entries.

- `closed: stdin` in red color for the closed stdin.
- `stdout="/dev/null"` in yellow color, meaning the stdout fd is modified and the current value is `/dev/null`.
- `4="/dev/random"` in green color, indicating a new fd numbered `4` pointing to `/dev/random`.

By default, `tracexec` hides the file descriptors marked `O_CLOEXEC` that will be closed upon exec.
To show such file descriptors, use `--no-hide-cloexec-fds` option. As demonstrated in the following example,
a python script opened a file descriptor with `O_CLOEXEC`, which gets closed when executing the shell.
tracexec shows the file descriptor in red color as `cloexec: 3="/"`.

<!-- asciinema record --window-size 80x8 --overwrite --command "tracexec log --no-hide-cloexec-fds -- python -c \"import os; fd = os.open('/', os.O_PATH | os.O_CLOEXEC); os.system('ls /')\"" ~/repos/tracexec/book/casts/log-fd-show-cloexec.cast -->

{{ #asciinema ../casts/log-fd-show-cloexec.cast opts=casts/autoplay.json }}

## Log Output Destination

By default, the log frontend outputs to `stderr`.

To output to `stdout`, use `--output -` or `-o-` (e.g. `target/debug/tracexec log -o- -- ls`).
To output to a file, use `--output <PATH>` where `<PATH>` is the path to the file for output.
tracexec will truncate the file if it already exists.

## (EXPERIMENTAL) Reconstruct Shell Commandline with File Descriptors

Previously, we showed how to reconstruct shell commandlines and track inherited file descriptors.
We can combine them to reconstruct a full shell commandline with the file descriptors.

To reconstruct the commandline with stdio descriptors, use `--stdio-in-cmdline`:

```bash
tracexec log --show-cmdline --stdio-in-cmdline -- bash -c "cat <&- > /dev/null 4</dev/random"
```

<!-- asciinema record --window-size 80x8 --overwrite --command "tracexec log --show-cmdline --stdio-in-cmdline -- bash -c \"cat <&- > /dev/null 4</dev/random\"" ~/repos/tracexec/book/casts/log-fd-stdio-cmd.cast -->

{{ #asciinema ../casts/log-fd-stdio-cmd.cast opts=casts/autoplay.json }}

To reconstruct the commandline with all file descriptors, use `--fd-in-cmdline`:


```bash
tracexec log --show-cmdline --fd-in-cmdline -- bash -c "cat <&- > /dev/null 4</dev/random"
```

<!-- asciinema record --window-size 80x8 --overwrite --command "tracexec log --show-cmdline --fd-in-cmdline -- bash -c \"cat <&- > /dev/null 4</dev/random\"" ~/repos/tracexec/book/casts/log-fd-fd-cmd.cast -->

{{ #asciinema ../casts/log-fd-fd-cmd.cast opts=casts/autoplay.json }}

This feature is currently experimental. It may produce inaccurate command lines.
For example,
- in the above example, the reconstructed cmdline shows FD 4 as both readable and writable, but actually we only used it as an input file descriptor.
- Shells like `zsh` have limit on the file descriptor number that could be used in the cmdline. Thus the reconstructed cmdline may not work in some shells.
  For instance,
    - `tracexec log -- bash -c "ls 114514</dev/null"` succeeds,
    - but `tracexec log -- zsh -c "ls 114514</dev/null"` failed with `ls: cannot access '114514': No such file or directory`

