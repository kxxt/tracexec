# Comparison with execsnoop(bpftrace)

[bpftrace](https://github.com/bpftrace/bpftrace) is a high-level tracing language that
compiles to eBPF. An [`execsnoop.bt`] script is shipped with this package on many Linux distributions (for example, `/usr/share/bpftrace/tools/execsnoop.bt` on Arch Linux).

This article compares tracexec with the latest commit `93b3247` of [`execsnoop.bt`] at the time of writing. Feel free to improve it if you found anything outdated.

[`execsnoop.bt`]: https://github.com/bpftrace/bpftrace/blob/master/tools/execsnoop.bt

## Shortcomings of execsnoop.bt

### Missing exec result

The script is only monitoring syscall entry and thus unable to report
whether or not the execs are successful.

### Missing details

The script is minimalistic and cannot show the filename, environment variables and the inherited file descriptors.

### Cannot Copy-Paste-Execute

A handy feature of tracexec is to copy the shell escaped command line to clipboard,
which you can directly paste into another terminal and hit enter to execute it.

But as for [`execsnoop.bt`]. It doesn't even quote the arguments,
making it hard to distinguish the boundary between arguments.

### Dependency Bloat

Although [`execsnoop.bt`] is minimalistic, the dependencies are not.
It depends on `bpftrace`, which in turn depends on both `clang` and `bcc`,
where the latter already includes their own implementation of [execsnoop](./bcc-execsnoop.md).
