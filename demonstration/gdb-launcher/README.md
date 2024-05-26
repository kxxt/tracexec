# GDB Launcher

This example demonstrates how to use tracexec as a gdb launcher to debug programs under complex setup.

To run this example, first ensure that tracexec and rust is installed on your system.

Then run `make` to compile the two simple rust programs.

In order to allow gdb to attach to the detached and stopped tracees, you probably need to run:

```bash
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope
```

On a machinne with Wayland/X11 display, assuming you have konsole installed(if not, please change the default-external-command), run

```bash
tracexec tui -t -b sysexit:in-filename:/a -b sysexit:in-filename:/b --default-external-command "konsole -e gdb -p {{PID}}" -- ./shell-script
```

or on a headless server, inside a tmux session, run:

```
tracexec tui -t -b sysexit:in-filename:/a -b sysexit:in-filename:/b --default-external-command "tmux split-window 'gdb -p {{PID}}'" -- ./shell-script
```

Alternatively, launch tracexec tui with a bash session and set the breakpoints in the TUI then run `./shell-script` in it.
