# Platform Support

Currently tracexec only supports Linux operating system.
Because the core of tracexec is implemented via ptrace, seccomp-bpf and eBPF,
it is difficult to port to Windows, MacOS or other operating systems.
(Well, technically speaking, ptrace itself is enough for initializing a port to
other operating systems, but ptrace without seccomp-bpf is painfully slow.)

## Architecture Support Status

Currently we support the following three architectures.
You are welcome to submit PR for supporting more architectures.

| Architecture       | Operating System | ptrace backend | ptrace backend <br> w/ seccomp-bpf | eBPF backend |
|--------------------|------------------|:--------------:|:----------------------------------:|:------------:|
| x86_64             | Linux            | ✅             | ✅                                 | ✅           |
| aarch64            | Linux            | ✅             | ✅                                 | ✅           |
| riscv64<sup>*</sup>| Linux            | ✅             | ✅                                 | ✅           |

**\***: for riscv64, some kernel versions has bugs in the ptrace implementation that would cause tracexec to display some information
as errors. See [this strace issue](https://github.com/strace/strace/issues/315) and
[the kernel mailing list discussion](https://lore.kernel.org/linux-riscv/20230801141607.435192-1-CoelacanthusHex@gmail.com/)
for more details if you got errors when using tracexec on riscv64.

## Linux Kernel Support Status

| Kernel Version | ptrace backend | eBPF backend | Comments |
|:--------------:|----------------|--------------|----------|
| \< 5.3         | ❌ (Need `PTRACE_GET_SYSCALL_INFO`) | ❌ | Seriously, upgrade your kernel!!!|
| >= 5.3,\< 5.17 | ✅ | ❌ (Need `bpf_loop`) |
| >=5.17         | ✅ | ✅ | |
| >= 6.2         | ✅ | ✅ | |
| (LTS) >=6.6.64, \<6.6.70 | ✅ | ❌ fail due to [kernel regression](https://lore.kernel.org/all/k32rq5abffq5kss5ejrzj3yx2dgn4c2ken2hrudws52mwuua4k@j64qawub3icu/)| Kernel regression caught by our CI |
