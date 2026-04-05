# Use tracexec as debugger launcher

Without tracexec, it's not trivial or convenient to debug a program that gets executed by other programs or debug programs with pipes:

- <https://stackoverflow.com/questions/5048112/use-gdb-to-debug-a-c-program-called-from-a-shell-script>
- <https://stackoverflow.com/questions/1456253/gdb-debugging-with-pipe>
- <https://stackoverflow.com/questions/455544/how-to-load-program-reading-stdin-and-taking-parameters-in-gdb>
- <https://ftp.gnu.org/old-gnu/Manuals/gdb/html_node/gdb_25.html>
- <https://stackoverflow.com/questions/65936457/debugging-a-specific-subprocess>
- <https://sourceware.org/gdb/current/onlinedocs/gdb.html/Forks.html>

This example demonstrates how to use tracexec as a gdb launcher to debug programs under complex setup.
The following video demonstrates the whole process:

<video src="https://github.com/kxxt/tracexec/assets/18085551/72c755a5-0f2f-4bf9-beb9-98c8d6b5e5fd" controls preload="none" loading="lazy" poster="../assets/gdb-launcher-cover.jpg" width="100%"></video>

To run this example, first ensure that tracexec and rust is installed on your system.

Clone the tracexec repository and enter the directory for this example:

```bash
git clone https://github.com/kxxt/tracexec
cd tracexec/book/tutorials/debugger-launcher
```

Then run `make` to compile the two simple rust programs.

In order to allow gdb to attach to the detached and stopped tracees, you probably need to run:

```bash
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope
```

On a machine with Wayland/X11 display, assuming you have konsole installed(if not, please change the default-external-command), run

```bash
tracexec tui -t \
    -b sysexit:in-filename:/a \
    -b sysexit:in-filename:/b \
    --default-external-command "konsole -e gdb -ex cont -ex cont -p {{PID}}" \
    -- ./shell-script
```

or on a headless server, inside a tmux session, run:

```bash
tracexec tui -t \
    -b sysexit:in-filename:/a \
    -b sysexit:in-filename:/b \
    --default-external-command "tmux split-window 'gdb -ex cont -ex cont -p {{PID}}'" \
    -- ./shell-script
```

Alternatively, launch tracexec tui with a bash session and set the breakpoints in the TUI then run `./shell-script` in it.


When the breakpoint get hit, open the Hit Manager and launch the external command for the two stopped tracees.
Then two gdb session will open.

To restart the tracees in gdb, Send command `c` twice. 
