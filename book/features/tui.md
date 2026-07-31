# Terminal User Interface

The Terminal User Interface (TUI) of tracexec supports most of features and provides an interactive way for tracing exec events.

By default, the TUI uses a built-in terminal pane for the tracee.

For example, here is how it usually looks like.

```bash
# Ptrace backend
tracexec tui -- env -u LANG A=B ls
# eBPF backend
tracexec --elevate ebpf tui -- env -u LANG A=B ls
```

<!-- asciinema record --window-size 100x26 --overwrite --command "tracexec tui -- env -u LANG A=B ls" ~/repos/tracexec/book/casts/tui-ls.cast -->

{{ #asciinema ../casts/tui-ls.cast opts=casts/autoplay-loop.json }}

The TUI is designed to be intitutive, that is, you should be able to use it without reading the rest of this docs.
It shows available actions at the bottom. When focused on the event list, press <kbd>F1</kbd> key to view the help within the TUI.

But if you like reading docs, feel free to continue.