# Event Details

The events list gives you a quick overview of what was executed.
When you want to know more about a particular event, select it in the `Events` pane
and press <kbd>V</kbd> to open its details.
If the `Terminal` pane is focused, use <kbd>Ctrl</kbd>+<kbd>S</kbd> to switch panes first.

For exec events, the details popup has three tabs: `Info`, `Environment` and `FdInfo`.
Other events, such as warnings, only have the `Info` tab.

## Navigation

- Use <kbd>←</kbd>/<kbd>→</kbd> to switch tabs, or <kbd>Tab</kbd> to cycle through them.
- Use <kbd>↑</kbd>/<kbd>↓</kbd> to scroll up/down.
- Use <kbd>PgUp</kbd>/<kbd>PgDn</kbd> to scroll a page at a time.
- Use <kbd>Home</kbd>/<kbd>End</kbd> to jump to the top or bottom of the current tab.
- Press <kbd>Q</kbd> to close the popup.

The following recording opens an exec event, selects a field, scrolls through its details
and uses <kbd>U</kbd> to view the parent event.

<!-- asciinema record --window-size 100x26 --overwrite --command "tracexec --no-profile -C / tui --active-pane events --layout vertical --filter exec -- /usr/bin/env -C /tmp /usr/bin/printf '%s\n' 'hello world'" book/casts/tui-details-info.cast -->

{{ #asciinema ../../casts/tui-details-info.cast opts=casts/autoplay-loop.json }}

## Info Tab

The `Info` tab shows the command line and the information collected about the exec call.
The fields include:

| Field | Description |
| --- | --- |
| `Timestamp` | When the event occurred. |
| `Duration` | The time from the event to process exit or detach, when available. |
| `Cmdline` | The reconstructed command line. |
| `Pid` | The process ID associated with the event. |
| `Exec Syscall` | Whether the program was executed with `execve` or `execveat`. |
| `Exec Pid` | The ID of the thread that made the exec call. If it differs from `Pid`, it is an exec from non-main thread, marked with `(non-main thread)`. |
| `Syscall Result` | `0 (Success)` for a successful exec, or the error returned by the syscall. |
| UID / GID fields and `Supplemental Groups` | The process credentials, with user and group names where available. |
| `Cgroup` | The cgroup v2 path, if collected. |
| `Process Status` | The process status known to tracexec when you opened the details. |
| `Cwd` | The working directory at exec. |
| `Comm (Before exec)` | The process name before the exec call. |
| `Filename` | The filename recorded for the executable. |
| `Interpreters` | Interpreter information, when available. |
| `Stdin`, `Stdout`, `Stderr` | The paths of the standard file descriptors when exist, or `Closed`. |
| `Argv` | The arguments in list form, including `argv[0]`. |

`Syscall Result` tells you whether the exec call succeeded. To see how the program ended,
look at `Process Status`. Similarly, `Duration` is not the time spent inside the exec syscall.
If the process is still running, close and reopen the popup later to see its updated status.

To collect the `Cgroup` field, start tracexec with `--collect-cgroup`:

```bash
tracexec tui --collect-cgroup -- bash
```

The two experimental command line fields try to include shell redirections for stdio
or all file descriptors. They can help you understand the setup, but commands involving
pipes or sockets may not be runnable as shown.

### Copying a Field

In the `Info` tab, press <kbd>W</kbd>/<kbd>S</kbd> to select the previous/next field.
The selected field is highlighted and marked with an arrow.
Press <kbd>C</kbd> to copy its value to the system clipboard, when clipboard access is available.

Scrolling and field selection are separate: <kbd>↑</kbd>/<kbd>↓</kbd> move the view,
while <kbd>W</kbd>/<kbd>S</kbd> change which value will be copied.
To copy the environment or a reconstructed command line directly from the events list,
see [Copy](copy.md).

### Viewing the Parent Event

If the parent event is still available, its command line appears in the `Info` tab.
`Parent(Spawner)` means a process spawned a child to execute this program.
`Parent(Becomer)` means the same process replaced its previous program with this one.

Press <kbd>U</kbd> from any tab to open the parent event's details.
This is useful when you want to work backwards through a script or build process.
Parent details are only available for events that tracexec has recorded and still keeps
in the events list. See [Backtrace](backtrace.md) for more about these relationships.

## Environment Tab

The `Environment` tab shows the environment passed to the exec call, with changes
relative to tracexec's starting environment:

- `+` marks an added variable or the new value of a modified variable.
- `-` marks a removed variable or the old value of a modified variable.
- A leading space marks an unchanged variable.

For a modified variable, the old value appears first and the new value follows it.
Unchanged variables appear after the changes.
All events use the same baseline, so this is not a recursive diff.

In this example, `env` adds `GREETING`, removes `LANG` and changes `DEMO_MODE`
from `before` to `after` before executing `true`.

<!-- env LANG=C.UTF-8 DEMO_MODE=before asciinema record --window-size 100x26 --overwrite --command "tracexec --no-profile -C / tui --active-pane events --layout vertical --filter exec -- /usr/bin/env -u LANG DEMO_MODE=after GREETING=hello /usr/bin/true" book/casts/tui-details-env.cast -->

{{ #asciinema ../../casts/tui-details-env.cast opts=casts/autoplay-loop.json }}

## File Descriptors Tab

The `FdInfo` tab shows the file descriptors collected at exec.
Each entry includes its number, path, flags, mount information, file position and inode number.
Some descriptors also have extra information, depending on their type.

Descriptors `0`, `1` and `2` are standard input, output and error.
Their targets can help explain why output went to a file or pipe instead of your terminal.
Other descriptors can reveal files that a parent process left open for the new program.

Here, `cat` has its stdin and stdout redirected to `/dev/null`, and an extra descriptor
`3` open for reading from `/dev/zero`. Switch to `FdInfo` and scroll down to see it.

<!-- asciinema record --window-size 100x26 --overwrite --command "tracexec --no-profile -C / tui --active-pane events --layout vertical --filter exec -- /usr/bin/bash --noprofile --norc -c 'exec /usr/bin/cat </dev/null >/dev/null 3</dev/zero'" book/casts/tui-details-fds.cast -->

{{ #asciinema ../../casts/tui-details-fds.cast opts=casts/autoplay-loop.json }}

By default, tracexec hides descriptors marked close-on-exec, since they are closed
when exec succeeds. To include them, use `--no-hide-cloexec-fds`:

```bash
tracexec tui --no-hide-cloexec-fds -- bash
```

The environment and file descriptor tabs show data collected for that exec event;
they do not track later changes made by the running program.
If tracexec could not collect a value, the popup shows the error or an unavailable marker.
