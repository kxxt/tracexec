# Event System

Backends and frontends communicate through `TracerMessage` values on a Tokio
unbounded MPSC channel. The channel keeps backend code independent from TUI,
log, and exporter rendering while giving every frontend the same exec model.

```text
ptrace backend ─┐
                ├─ TracerMessage channel ─┬─ log printer
eBPF backend ───┘                         ├─ TUI event list
                                          └─ JSON / Perfetto exporter

TUI ── PendingRequest channel ──> ptrace backend (breakpoint control only)
```

## Message classes

`TracerMessage` has three variants:

- `Event(TracerEvent)` is a record that may get its own line in log or TUI
  output. Each record has a monotonically allocated `EventId` and a
  `TracerEventDetails` payload.
- `StateUpdate(ProcessStateUpdateEvent)` changes the state of existing records
  without adding a new line. Exit, breakpoint hit, resume, detach, and related
  errors use this path.
- `FatalError(String)` tells the frontend that the backend cannot continue.

Keeping state updates separate matters in the TUI. One process exit can update
the status of several exec records for that process; rendering a second
standalone line would lose that relationship.

## Event payloads

`TracerEventDetails` currently represents:

- informational, warning, and error messages;
- discovery of a new child;
- an exec attempt and its inspected process state;
- root tracee spawn and exit lifecycle records.

`ExecEvent` is the main payload. It contains syscall kind and result, process
identities, filename, argv, environment, working directory, credentials,
interpreter chain, descriptor table, timestamp, cgroup information, and an
optional parent event link.

Many fields are fallible. `OutputMsg::PartialOk` means some useful text was
recovered but not all of it; `OutputMsg::Err` means the value could not be
inspected. Compound fields use `Result`. New consumers should render or export
those states, not collapse them into defaults.

## Filtering

The `TracerEventDetailsKind` generated beside the details enum is used by
`--filter`, `--filter-include`, and `--filter-exclude`. Backends call
`send_if_match` before putting display events on the channel. The normal default
is `warning,error,exec,tracee-exit`.

State updates are not ordinary display events and must not be dropped by that
filter. A frontend may need an exit update even when it does not render exit
events, otherwise running statuses never settle.

When adding a new display event, decide whether it belongs in the default
filter. Also check help text, parser names, log formatting, TUI formatting, and
tests for include/exclude combinations.

## Exec parent links

An exec parent is an event relationship, not necessarily a Unix parent PID:

- `ParentEvent::Spawn(id)` means the process represented by `id` forked a child
  that later produced this exec event.
- `ParentEvent::Become(id)` means the same process represented by `id` replacing itself.

`ParentTracker` records the last successful exec. A failed exec can point to an
ancestor, but it does not replace that last-successful value. The TUI uses these
links for <kbd>U</kbd> parent navigation and the `S`/`B` markers in an exec
backtrace.

Links are IDs rather than references so events can move through channels and be
serialized cheaply.

## Frontend consumption

Frontends consume different subsets:

- log formats display events immediately;
- the TUI stores display events and applies state updates to its event list;
- JSON exporters keep exec events and use root tracee exit to finish the file;
- Perfetto converts exec and lifecycle messages into trace packets.
