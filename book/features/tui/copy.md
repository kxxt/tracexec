# Copy

When running in a desktop environment (X11 or Wayland), the TUI of tracexec supports copying details to the system clipboard.

<!-- asciinema record --window-size 100x26 --overwrite --command "target/debug/tracexec tui -- env -C / ls" ~/repos/tracexec/book/casts/tui-copy.cast -->

{{ #asciinema ../../casts/tui-copy.cast opts=casts/autoplay-loop.json }}

To use it, first select an event in the event list.
Then press <kbd>C</kbd> to open the copy popup.

After that, select an entry in the list and press <kbd>Enter</kbd>
to copy it to the clipboard. Alternatively, press the corresponding character of the entry to directly copy it.

## Available Copy targets

- (`c`) `Command line`: the reconstructed commandline
- (`o`) `Command line with full env`: the reconstructed commandline with full set of environment variables.
- *Experimental.* (`s`) `Command line with Stdio`: the reconstructed commandline with stdio file descriptors.
- *Experimental.* (`f`) `Command line with File descriptors`: the reconstructed commandline with file descriptors.
- (`e`) Environment Variables: environment variables in `"KEY"="VALUE"` format.
- (`d`) Diff of environment variables: diff of environment variables in the following format.

```text
# Added:
"KEY2"="VALUE2"
# Modified: (original first)
"PATH"="OLDPATH"
"PATH"="NEWPATH"
# Removed:
"KEY1"="VALUE1"
```

- (`a`) Arguments: `argv` in list format, e.g. `["/usr/bin/starship", "time"]`.
- (`w`) Arguments joined by whitespace: `argv` joined by whitespace.
- (`n`) Filename: the file name of the executable.
- (`r`) Syscall result: the result of the exec syscall
- (`l`) Current Line: the current entry as displayed in the TUI.
