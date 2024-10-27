# Install tracexec

## Requirements

tracexec can be installed on Linux systems and should work in kernels greater than 4.9.
(Kernels < 4.9 are untested, but probably works).

The eBPF feature should work on 6.x kernels.

## Install via Package Manager

[![Packaging status](https://repology.org/badge/vertical-allrepos/tracexec.svg)](https://repology.org/project/tracexec/versions)

Arch Linux users can install from the official repositories via `pacman -S tracexec`.

## Install From Source

To install from source, the following dependencies are needed:

- A working rust compiler and `cargo`.
- `libbpf`: if not using `vendored-libbpf`
- `zlib`: if not using `vendored-zlib`
- `libelf`: if not using `vendored-libelf`
- `libseccomp`: For `seccomp-bpf` feature.
- If any library vendoring feature is enabled:
  - `build-essential` `autopoint` `gettext` for Debian based distros
  - `base-devel` for Arch Linux
- `clang` for compiling eBPF program.

### Library Linkage

By default, we dynamically link to libseccomp. In order to statically link to libseccomp,
please set `LIBSECCOMP_LINK_TYPE` to `static` and set `LIBSECCOMP_LIB_PATH` to the path of
the directory containing `libseccomp.a`.

To control whether to dynamically link to libbpf, libelf and zlib, consult the next `Feature Flags` section.

### Feature Flags

- `recommended`: This enables the recommended functionalities of tracexec
    - `seccomp-bpf`: Use seccomp to accelerate ptrace operations. (Things are extremely slow if this is turned off.)
    - `ebpf`: eBPF backend that doesn't use ptrace and could be used for system wide tracing
- `ebpf-debug`: Not meant for end users. This flag enables debug logging to `/sys/kernel/debug/tracing/trace_pipe` and some debug checks.
- `static`: Statically link libelf, zlib and libbpf.
- `vendored`: Vendoring libelf, zlib and libbpf, implies `static`.
- `vendored-libbpf`: Vendoring libbpf and statically link to it.
- `ebpf-no-rcu-kfuncs`: Enable this feature for eBPF backend to work on kernel versions less than `6.2`.

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
