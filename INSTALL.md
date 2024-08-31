# Install tracexec

## Requirements

tracexec can be installed on Linux systems and should work in kernels greater than 4.9.
(Kernels < 4.9 are untested, but probably works).

The eBPF feature should work on 6.x kernels.

## Install via Package Manager

[![Packaging status](https://repology.org/badge/vertical-allrepos/tracexec.svg)](https://repology.org/project/tracexec/versions)

Arch Linux users can also install from the official repositories via `pacman -S tracexec`.

## Install From Source

### Feature Flags

- `recommended`: This enables the recommended functionalities of tracexec
    - `seccomp-bpf`: Use seccomp to accelerate ptrace operations. (Things are extremely slow if this is turned off.)
    - `ebpf`: eBPF backend that doesn't use ptrace and could be used for system wide tracing
- `ebpf-debug`: Not meant for end users. This flag enables debug logging to `/sys/kernel/debug/tracing/trace_pipe` and some debug checks.
- `static`: Statically link libelf, zlib and libbpf.
- `vendored`: Vendoring libelf, zlib and libbpf, implies `static`.
- `vendored-libbpf`: Vendoring libbpf and statically link to it.

By default, we enable the `recommended` and `vendored-libbpf` features. This means that we are dynamically linking zlib and libelf but statically linking libbpf. This choice is made because zlib and libelf are usually installed on most systems but libbpf is usually not.

To dynamically link to libbpf, turn off default features and enable `recommended` feature:

```bash
cargo build --release --no-default-features -F recommended
```

### Install via Cargo

```bash
cargo install tracexec --bin tracexec
```

## Prebuilt Binary

You can download the binary from the [release page](https://github.com/kxxt/tracexec/releases)
