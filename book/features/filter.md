# Filtering

In tracexec, we currently provide two mechanisms for filtering events.

## Only Show Successful Execs

In many cases, you may only want to get a trace of successful exec events because the failure cases are not interesting.
For example, many failure cases are just program trying every possible path in `PATH` environment variable until success.

To show only the successful exec events, use `--successful-only` option, as shown in the following example.

```bash
tracexec log --show-cmdline --successful-only -- env -u LANG A=B ls
```

<!-- asciinema record --window-size 80x10 --overwrite --command "tracexec log --show-cmdline --successful-only -- env -u LANG A=B ls" ~/repos/tracexec/book/casts/log-ls-cmdline-succ.cast -->

{{ #asciinema ../casts/log-ls-cmdline-succ.cast opts=casts/autoplay.json }}

## Event filter

Under default settings, tracexec tries to output a sensible amount of details to avoid overwhelming the user.

We provide a custom event filtering system for advanced users where the default settings fall short.

tracexec produces the following types of events:

- `info`, `warning`, `error`: a notification to the user.
- `new-child`: a new traced child process is observed.
- `exec`: exec event.
- `tracee-spawn`: the root tracee spawns. 
- `tracee-exit`: the root tracee terminates.

Please note that not all frontends display all the event types. Thus certain event types may not show in some frontends even if enabled in the filter.

By default, the filter enables `warning,error,exec,tracee-exit` events.

To enable additional events in the filter, use `--filter-include`.
For example, `tracexec tui --filter-include tracee-spawn -- ls` shows `tracee-spawn` event in addition to default events.

<!-- asciinema record --window-size 80x10 --overwrite --command "tracexec tui --filter-include tracee-spawn -- ls" ~/repos/tracexec/book/casts/tui-filter-include.cast -->

{{ #asciinema ../casts/tui-filter-include.cast opts=casts/autoplay-loop.json }}

To disable events in the filter, use `--filter-exclude`.
For example, with `tracexec tui --filter-exclude tracee-exit -- ls`, the `tracee-exit` event is hidden.


<!-- asciinema record --window-size 80x10 --overwrite --command "tracexec tui --filter-exclude tracee-exit -- ls" ~/repos/tracexec/book/casts/tui-filter-exclude.cast -->

{{ #asciinema ../casts/tui-filter-exclude.cast opts=casts/autoplay-loop.json }}

Alternatively, instead of manipulating the default filter with `--filter-include` and `--filter-exclude`, you can also set the default filter directly with `--filter`.
For example, `tracexec tui --filter exec,error -- ...` will only show exec and error events.
