# Basics

In this section, I will introduce the basics of the TUI of tracexec.

## Navigation

By default, the TUI comes with two panes: the `Events` pane and the `Terminal` pane.

{{ #asciinema ../../casts/tui-ls.cast opts=casts/autoplay-loop.json }}

The `Terminal` pane is focused upon launch but you can configure tracexec to focus the `Events` pane with a configuration file.

### Terminal Pane

If you launch the TUI to trace a shell, you can type commands in the `Terminal` pane and tracexec will show the exec events in the `Events` pane.

If you launch the TUI to trace a program, you can interact with it in the `Terminal` pane.

Use <kbd>Ctrl</kbd>+<kbd>U</kbd> shortcut to enter scrollback mode, in which you
can view the history of the terminal pane.
Use <kbd>↑</kbd>/<kbd>↓</kbd>/<kbd>PgUp</kbd>/<kbd>PgDn</kbd>/<kbd>Home</kbd>/<kbd>End</kbd> to navigate through the scroll buffer. Use <kbd>Ctrl</kbd>+<kbd>U</kbd> shortcut again to exit the scrollback mode.

<!-- asciinema record --window-size 100x26 --overwrite --command "target/debug/tracexec tui -- bash" ~/repos/tracexec/book/casts/tui-scrollback.cast -->

{{ #asciinema ../../casts/tui-scrollback.cast opts=casts/autoplay-loop.json }}

### Events Pane

Switch to the `Events` pane by shortcut <kbd>Ctrl</kbd>+<kbd>S</kbd>.

- Use <kbd>↑</kbd>/<kbd>↓</kbd> to scroll up/down the events list.
- Use <kbd>PgUp</kbd>/<kbd>PgDn</kbd>/<kbd>Ctrl</kbd>+<kbd>↑</kbd>/<kbd>Ctrl</kbd>+<kbd>↓</kbd> to scroll faster.
- Use <kbd>Home</kbd>/<kbd>End</kbd> to jump to the start and the end of the list.
- Use <kbd>←</kbd>/<kbd>→</kbd> to scroll left/right in the events list.
- Use <kbd>Ctrl</kbd>+<kbd>←</kbd>/<kbd>Ctrl</kbd>+<kbd>→</kbd> to scroll faster.
- Use <kbd>Shift</kbd>+<kbd>Home</kbd>/<kbd>Shift</kbd>+<kbd>End</kbd> to jump to the left end and the right end of the view.

<!-- asciinema record --window-size 100x26 --overwrite --command "target/debug/tracexec tui -- env -C / bash" ~/repos/tracexec/book/casts/tui-events-nav.cast -->

{{ #asciinema ../../casts/tui-events-nav.cast opts=casts/autoplay-loop.json }}


### How to Exit ~~Vim~~ Tracexec


When the `Events` pane is focused, press <kbd>Q</kbd> to exit.

You can switch to the `Events` pane by shortcut <kbd>Ctrl</kbd>+<kbd>S</kbd> if the `Terminal` pane is focused.

If there are still tracees running in the terminal pane, tracexec will wait for them after the TUI is closed. Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to terminate them.

## Layout

When the `Events` pane is focused,

- Press <kbd>Alt</kbd>+<kbd>L</kbd> to change between vertical and horizontal layout.
- Hold <kbd>G</kbd>/<kbd>S</kbd> to grow/shrink the `Events` pane.

<!-- asciinema record --window-size 100x26 --overwrite --command "target/debug/tracexec tui -- env -C / bash" ~/repos/tracexec/book/casts/tui-layout-nav.cast -->

{{ #asciinema ../../casts/tui-layout-nav.cast opts=casts/autoplay-loop.json }}

