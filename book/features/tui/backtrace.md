# Backtrace

The TUI can reconstruct the exec backtrace of an event. It is **NOT** a stack backtrace.

## Traversing the Backtrace in the Events Pane

Press <kbd>U</kbd> when focusing the `Events` pane to jump to the parent event of the selected event.

<!-- asciinema record --window-size 100x26 --overwrite --command "tracexec tui -- env -u LANG -C / A=B ls" ~/repos/tracexec/book/casts/tui-jump-parent.cast -->

{{ #asciinema ../../casts/tui-jump-parent.cast opts=casts/autoplay-loop.json }}

## View the Full Backtrace

Select an exec event in the `Events` pane and press <kbd>T</kbd> to show the full backtrace.
The popup lists the oldest available ancestor first and the selected event last. Its markers
distinguish two relationships:

- `S` (spawns): a process forked a child, and the child later executed the next
  program;
- `B` (becomes): the same process replaced its image with another program.

<!-- asciinema record --window-size 100x26 --overwrite --command "tracexec tui -- env -u LANG -C / A=B bash -c 'ls;ps'" ~/repos/tracexec/book/casts/tui-backtrace.cast -->

{{ #asciinema ../../casts/tui-backtrace.cast opts=casts/autoplay-loop.json }}

## Incomplete backtraces

Parent links refer to earlier event IDs kept by the TUI. A popup is marked
`incomplete` when an ancestor has already been discarded because
`--max-events` was reached. Increase the limit, or use `--max-events 0` for an
unlimited list when retaining the full lineage matters.

Failed exec attempts appear in the event list but do not replace the last
successful program image. They therefore do not become ancestors of later
successful execs.
