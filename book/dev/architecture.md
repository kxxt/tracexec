# Internal Architecture

`tracexec` maintains several frontends and backends through a unified event system.

## Crates

For modularity, `tracexec` consists of several crates.
Generally speaking, most of them can be divided into two categories: frontend crates and backend crates.

Frontend crates handles the presentation of the information to the users,
while backend crates handles the collection of information.

Currently, there are three frontend crates:

- `tracexec-tui`
- `tracexec-exporter-json`
- `tracexec-exporter-perfetto`
- There's no separate crate for the `log` frontend, which instead lives in the `tracexec-core` crate.

And there are two backend crates:

- `tracexec-backend-ptrace`
- `tracexec-backend-ebpf`

Additionally, the `tracexec-core` crate consists of abstractions and primitives that are used through out
all the above crates.

The `perfetto-trace-proto` crate is an optional dependency for the `tracexec-exporter-perfetto` crate.
We includes a tiny perfetto trace protobuf binding minified by hand so `perfetto-trace-proto`
is not used by default.

All the crates are internal implementation details even though they are published on crates.io.
They shouldn't be introduced as a dependency in other projects.
But in case you want to re-use some code from tracexec, feel free to open a discussion.
We may separate the re-useable parts into a new crate.

## Event System

The event system is the heart of tracexec (I mean it is the core part of the circulatory system in tracexec).
It receives incoming new events from the backend and routes them to the frontend.

`TracerMessage` struct is the actual type for event messages going from the backend to frontend. 
The frontend can also send `PendingRequest` struct to the backend but it is currently only used in `ptrace` backend.

## Frontend Architecture

There is currently no abstraction layer for frontends as different frontends have dramatically different capabilities (e.g. TUI v.s. logging).

But we do have an `Exporter` trait that serves as an abstraction for exporter frontends that converts events to certain output formats.

## Backend Architecture

There's no unified abstraction for backends.
We only have a shared `TracerBuilder` that could be used to configure properties that are shared across multiple backends.