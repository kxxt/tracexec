# Build from Source

To build tracexec from source, the following dependencies are needed:

- A working rust compiler and `cargo`.
  - Refer to `package.rust-version` in `Cargo.toml` for MSRV.
- `libbpf`: if not using `vendored-libbpf`
- `zlib`: if not using `vendored-zlib`
- `libelf`: if not using `vendored-libbpf`
- `libseccomp`: For `seccomp-bpf`.
- If any library vendoring feature is enabled:
  - `build-essential` `autopoint` `gettext` for Debian based distros
  - `base-devel` for Arch Linux
- `protoc` for compiling ProtoBuf `proto` files if `protobuf-binding-from-source` feature is enabled.
  - By default, `protoc` from `PATH` is used. `PROTOC` environment variable
    could be used to specify the **full** path to the desired protoc compiler.
- `clang` for compiling eBPF program.
  - By default, `clang` from `PATH` is used. `CLANG` environment variable
    could be used to specify the **full** path to the desired clang compiler.

## Library Linkage

By default, we dynamically link to libseccomp because most distros ship it out of box.
In order to statically link to libseccomp,
please set `LIBSECCOMP_LINK_TYPE` to `static` and set `LIBSECCOMP_LIB_PATH` to the path of
the directory containing `libseccomp.a`.

To control whether or not to dynamically link to libbpf, libelf and zlib, consult the next `Feature Flags` section.

## Feature Flags

- `recommended`: This enables the recommended functionalities of tracexec
    - `ebpf`(experimental): eBPF backend that doesn't use ptrace and could be used for system wide tracing
- `ebpf-debug`: Not meant for end users. This flag enables debug logging to `/sys/kernel/debug/tracing/trace_pipe` and some debug checks.
- `static`: Statically link libelf, zlib and libbpf.
- `vendored`: Vendoring libelf, zlib and libbpf, implies `static`.
- `vendored-libbpf`: Vendoring libbpf and statically link to it.

By default, we enable the `recommended` and `vendored-libbpf` features. This means that we are dynamically linking zlib and libelf but statically linking libbpf. This choice is made because zlib and libelf are usually installed on most systems but libbpf is usually not.

To dynamically link to libbpf, turn off default features and enable `recommended` feature:

```bash
cargo build --release --no-default-features -F recommended
```

