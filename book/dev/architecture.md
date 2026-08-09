# Internal Architecture

`tracexec` maintains several frontends and backends through a unified event system.

## Crates

For modularity, `tracexec` consists of several crates.
Generally speaking, most of them can be divided into two categories: frontend crates and backend crates.

Frontend crates handle presentation, while backend crates handle collection.

Currently, there are three dedicated frontend crates:

- `tracexec-tui`
- `tracexec-exporter-json`
- `tracexec-exporter-perfetto`

There is no separate crate for the `log` frontend; it lives in the `tracexec-core` crate.

And there are two backend crates:

- `tracexec-backend-ptrace`
- `tracexec-backend-ebpf`

Additionally, the `tracexec-core` crate consists of abstractions and primitives that are used throughout
all the above crates.

The `perfetto-trace-proto` crate is an optional dependency for the `tracexec-exporter-perfetto` crate.
We include a tiny perfetto trace protobuf binding minified by hand so `perfetto-trace-proto`
is not used by default.

All the crates are internal implementation details even though they are published on crates.io.
They shouldn't be introduced as a dependency in other projects.
If you want to reuse code from tracexec, open a discussion. We may separate the
reusable parts into a supported crate.

## Event System

The event system receives records from a backend and routes them to the selected
frontend. `TracerMessage` is the channel payload. The ptrace TUI also sends
`PendingRequest` values in the other direction for ptrace control, including
breakpoint actions, seccomp-BPF suspension, and tracer termination.

See [Event System](./event-system.md) for the message variants, filtering, and
exec parent relationships.

## Frontend Architecture

There is currently no common abstraction for frontends because the TUI, log
mode, and exporters have different lifecycle and interaction requirements.

The `Exporter` trait covers the narrower case of converting an event stream to
a structured output format.

## Backend Architecture

There is no unified abstraction for backends.
`TracerBuilder` configures the properties shared by multiple backends, but each
backend owns its build and run path.

See [Backend Differences](./backend-differences.md) before changing shared
collection behavior.
