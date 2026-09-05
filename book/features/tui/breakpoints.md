# Breakpoints

tracexec supports setting breakpoints at exec syscall enter/exit stops,
which enables you to pause programs before they start to execute user-space code.

This feature is only available for the ptrace backend.

## Why

You might be wondering why such a feature is useful.
It is mainly because of a limitation of the `ptrace(2)` API.
A single tracee can only have one tracer at any time.
As a result, debuggers like `gdb` cannot be used on processes
traced by tracexec.

This feature provides a way to hand over processes traced by tracexec to other debuggers.
For example, you can stop a program launched deep inside a shell script and attach gdb to it,
with its environment, working directory and pipes already set up.

## Breakpoint Stops

There are two places where you can set a breakpoint:

- `sysenter`: right before the exec syscall. The process still has its old program image.
- `sysexit`: right after the exec syscall. If the exec succeeded, the new program is loaded
  but has not started running user-space code yet.

## Breakpoint Patterns

A breakpoint pattern decides which exec calls to stop at.
There are three kinds of patterns:

| Pattern | When it matches | Example |
| --- | --- | --- |
| `in-filename` | The filename contains the given string. | `in-filename:/my-program` |
| `exact-filename` | The filename is exactly the given string. | `exact-filename:./my-program` |
| `argv-regex` | The arguments, joined by spaces, match the regular expression. | `argv-regex:^my-program --verbose( \|$)` |

The filename patterns match the filename recorded by tracexec. The filename is typically the
exact value used in `execve` syscall or the resolved path of the value used in `execveat` syscall.

For `argv-regex`, the arguments include `argv[0]` and are joined without any quoting or escaping.
For example, `["echo", "hello world"]` becomes `echo hello world`.
The regex can match anywhere in that string; use `^` and `$` if you want to match the whole string.

## Setting Breakpoints from the Command Line

Use `-b` (or `--add-breakpoint`) to add a breakpoint before tracing starts.
The format is `<breakpoint-stop>:<pattern-kind>:<pattern>`.

For example, to stop whenever a program whose filename contains `/my-program` is executed:

```bash
tracexec tui -b 'sysexit:in-filename:/my-program' -- bash
```

You can now run `./my-program` in the terminal pane, either directly or through a script.
It will stop before it starts running, and tracexec will show a hit at the bottom of the screen.

You can use `-b` multiple times to add more breakpoints:

```bash
tracexec tui \
    -b 'sysexit:exact-filename:./a' \
    -b 'sysexit:exact-filename:./b' \
    -- ./shell-script
```

Quote the breakpoint when it contains spaces or shell special characters, especially for regex patterns.

## Setting Breakpoints in the TUI

When the `Events` pane is focused, press <kbd>B</kbd> to open the `Breakpoint Manager`.
If the `Terminal` pane is focused, use <kbd>Ctrl</kbd>+<kbd>S</kbd> to switch panes first.

Press <kbd>N</kbd> to create a new breakpoint. The editor accepts only the pattern,
such as `in-filename:/my-program`, without the `sysenter:` or `sysexit:` prefix.
Do not add a space after the colon unless you want that space to be part of the pattern.

New breakpoints are active and stop at `Syscall Exit` by default.
While editing:

- Press <kbd>Alt</kbd>+<kbd>S</kbd> to switch between `Syscall Enter` and `Syscall Exit`.
- Press <kbd>Alt</kbd>+<kbd>A</kbd> to toggle whether the breakpoint is active.
- Press <kbd>Enter</kbd> to save, or <kbd>Ctrl</kbd>+<kbd>C</kbd> to cancel.

In the breakpoint list, use <kbd>↑</kbd>/<kbd>↓</kbd> to select a breakpoint.
Press <kbd>Enter</kbd> or <kbd>E</kbd> to edit it, <kbd>Space</kbd> to enable or disable it,
or <kbd>Delete</kbd>/<kbd>D</kbd> to delete it.
Press <kbd>Q</kbd> to return to the events pane.

Disabling or deleting a breakpoint does not resume a process that has already hit it.

<!-- asciinema record --window-size 100x26 --overwrite --command "env -C / tracexec tui -- bash" ~/repos/tracexec/book/casts/tui-breakpoint.cast -->

{{ #asciinema ../../casts/tui-breakpoint.cast opts=casts/autoplay-loop.json }}

## Handling Breakpoint Hits

When a process hits a breakpoint, tracexec pauses that process and shows the number of hits
at the bottom of the screen. Other tracees can keep running, although they may be waiting
for the stopped process.

Press <kbd>Z</kbd> from the `Events` pane to open the `Hit Manager`.
Use <kbd>↑</kbd>/<kbd>↓</kbd> to select a stopped process, then:

- Press <kbd>R</kbd> to resume it and keep tracing it.
- Press <kbd>D</kbd> to detach and let it continue without tracexec tracing it.
- Press <kbd>Enter</kbd> to detach, leave it stopped and run the default external command.
- Press <kbd>Alt</kbd>+<kbd>Enter</kbd> to enter a command to run for this particular hit.

Press <kbd>Q</kbd> to close the Hit Manager. This leaves the processes stopped.
You can press <kbd>F1</kbd> in either manager to view its help.

## Launching a Debugger

See [Use tracexec as debugger launcher](../../tutorials/debugger-launcher.md) for a complete tutorial.

Set `--default-external-command` to the command you want to launch for a hit.
tracexec replaces `{{PID}}` with the PID of the detached and stopped process.
You can also set or edit this command by pressing <kbd>E</kbd> in the Hit Manager.

For example, if you use Konsole:

```bash
tracexec tui --seccomp-bpf=off \
    -b 'sysexit:in-filename:/my-program' \
    --default-external-command 'konsole -e gdb -p {{PID}}' \
    -- bash
```

Run your program in the terminal pane. When it hits the breakpoint, switch to the events pane,
press <kbd>Z</kbd>, select the hit and press <kbd>Enter</kbd>.
A new terminal will open with gdb attached to the process.
You may need to run `continue` twice in gdb because of the stop signal used during detach.

Use a terminal emulator or a command such as `tmux split-window` for an interactive debugger:
the external command's standard input, output and error are connected to `/dev/null`.
The command supports shell-style quoting, but is not run through a shell.
If you need shell features such as pipes or redirection, invoke a shell explicitly.

The `--seccomp-bpf=off` option matters if the detached process or its children need to exec
other programs. With the seccomp-bpf optimization enabled, those exec calls can fail with
`Function not implemented` after detach. Set this option when starting tracexec.
