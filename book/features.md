# Features

tracexec supports many features, which will be explained in detail in this chapter.

At a high level, tracexec has two backends and several frontends. A backend
collects exec events; a frontend decides how those events are presented or
stored.

The default [ptrace backend](./features/ptrace.md) follows a command and its
descendants. The [eBPF backend](./features/ebpf.md) can also trace execs across
the whole system. Both feed the same event model.

Choose a frontend based on the job:

- [Log](./features/log.md) prints events as they arrive and works well in a
  pipeline or CI log.
- [TUI](./features/tui.md) keeps an interactive event list next to the traced
  program's terminal.
- [Collect](./features/collect.md) writes JSON, NDJSON, or a Perfetto trace for
  later analysis.

[Filtering](./features/filter.md), privilege elevation, and most data-collection
options are shared across frontends.
