# Convenient Privilege Elevation

When using tracexec with eBPF backend or tracing setuid/setgid binaries with ptrace backend,
it usually requires running tracexec as root.
However, using sudo with tracexec is a little tricky because sudo manipulates the environment variables,
which might not be noticed by the user.

For example, when running `sudo tracexec ebpf log -- make -j$(nproc)`,

- `sudo` resets the environment variables for tracexec and the tracee by retaining
  a minimal set of basic environment variables and may override some important variables for security reasons (e.g. `PATH`).
- `sudo` inserts its own environment variables like `SUDO_USER`, `SUDO_UID` and `SUDO_COMMAND`.
- The tracee `make` is ran as root, which may not be desired.

In many cases, what we want to achieve is to run tracexec with root privilege
but still run the tracee in the original context as an unprivileged user.
The following command almost achieves it, with the caveat that `sudo -E` still modifies the environment variables.

```bash
sudo -E tracexec --user $(whoami) ebpf log -- make -j$(nproc)
```

Starting at tracexec 0.18.0, we offer a new CLI flag that conveniently runs tracexec as root but
runs tracee with the original user and environment variables.

For example, the following command runs tracexec as root but runs `make -j$(nproc)` as the original user:

```bash
tracexec --elevate ebpf log -- make -j$(nproc)
```

When using this feature, tracexec will internally use `sudo` for privilege elevation.
So sudo needs to be installed on your system and you may need to authenticate yourself
to sudo when tracexec executes sudo.
