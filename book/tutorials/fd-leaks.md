# Catching FD leaks

When spawning a subprocess using the `fork`/`exec` family syscalls,
file descriptors can be leaked to the subprocesses if the code does not mark them as `O_CLOEXEC` or close them after fork.
FD leaks can cause bugs or even security vulnerabilities.

In this tutorial, we will write a small launcher that accidentally passes its log file
to a worker, find the descriptor with tracexec and fix the leak.
You will need Linux, tracexec and a C compiler available as `cc`.
The worker is the external `printf` program, which should be available in your `PATH`.

Here is a recording of the example, the investigation and the fix:

<!-- From book/tutorials/fd-leaks, run: asciinema record --window-size 100x26 --overwrite --command "bash --noprofile --norc" ../../casts/fd-leaks.cast -->

{{ #asciinema ../casts/fd-leaks.cast opts=casts/autoplay-loop.json }}

## The Example Program

Our launcher opens `launcher.log`, writes a message and forks a child to run `printf`.
The parent closes the log and waits for the worker to finish.
That sounds reasonable, but there is a missing piece.

Create a directory for the example:

```bash
mkdir fd-leaks
cd fd-leaks
```

Save the following as `launcher.c`. The complete source is also available in
`book/tutorials/fd-leaks` in the tracexec repository.

```c
{{#include fd-leaks/launcher.c}}
```

Compile and run it:

```bash
cc -std=c11 -Wall -Wextra -o launcher launcher.c
./launcher
```

```text
Worker finished
```

It also appends `Starting worker` to `launcher.log`.
There is no error message, and the program exits successfully.
The problem is what the worker inherited along the way.

## Finding the Leaked Descriptor

Trace the launcher:

```bash
tracexec tui -- ./launcher
```

You should see an exec event for `launcher`, followed by one for `printf`.
Switch to the `Events` pane with <kbd>Ctrl</kbd>+<kbd>S</kbd>, select the successful
`printf` event and press <kbd>V</kbd> to open its details.
Depending on your `PATH`, there may be failed attempts to find `printf` before the successful one.

Press <kbd>→</kbd> twice to switch to `FdInfo`, then <kbd>End</kbd> to scroll to the bottom.
Alongside stdin, stdout and stderr, you will find another descriptor pointing to
`launcher.log`. It is descriptor `3` in the recording, but the exact number can differ.

The entry shows the full path and flags such as `O_WRONLY` and `O_APPEND`.
This is the launcher's log, yet it appears in the worker's exec event.
`printf` has no reason to use this file descriptor thus it is a leak.

## Fixing the Leak

Press <kbd>Q</kbd> to close the details, then <kbd>Q</kbd> again to leave tracexec.
Add `O_CLOEXEC` to the flags passed to `open`:

```c
int log_fd = open("launcher.log", O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
```

This marks the descriptor to be closed automatically when the child successfully executes
the worker. It can still be used to write the log before exec.

The recording makes this edit with:

```bash
sed -i 's/O_APPEND,/O_APPEND | O_CLOEXEC,/' launcher.c
```

Recompile and trace the program again:

```bash
cc -std=c11 -Wall -Wextra -o launcher launcher.c
tracexec tui -- ./launcher
```

Open the successful `printf` event and check `FdInfo` again.
The `launcher.log` entry is gone and thus the leak is solved.
