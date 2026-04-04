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

| Architecture | Kernel Version | ptrace backend | eBPF backend | Comments |
|--------------|:--------------:|----------------|--------------|----------|
| all          | \< 5.3         | ❌ (Need `PTRACE_GET_SYSCALL_INFO`)   | ❌ | Seriously, upgrade your kernel!!!     |
| all          | >= 5.3,\< 5.17 | ✅             | ❌ (Need `bpf_loop`) |                                            |
| x86_64       | >=5.17         | ✅             | ✅                   |                                            |
| aarch64      | >=5.17,\< 5.18 | ✅             | ❌ (No BPF atomics)  |                                            |
| riscv64      | >=5.17,\< 5.19 | ✅             | ❌ (No BPF atomics)  |                                            |
| riscv64      | >=5.19,\< 6.1  | ✅             | 🚨 (Buggy kernel)    | The eBPF backend may trigger kernel bug.   |
| aarch64      | >=5.18         | ✅             | ✅                   |                                            |
| riscv64      | >=6.1,\< 6.19  | ✅             | ✅                   |                                            |
| riscv64      | >= 6.19        | ✅             | ❌ (Kernel bug)      | task_local_storage is not working properly |
| all          | (LTS) >=6.6.64, \<6.6.70 | ✅ | ❌ fail due to [kernel regression](https://lore.kernel.org/all/k32rq5abffq5kss5ejrzj3yx2dgn4c2ken2hrudws52mwuua4k@j64qawub3icu/)| Kernel regression caught by our CI |

## LLVM Support Status

tracexec requires [`clang`](https://clang.llvm.org/) from LLVM for building the eBPF backend.
We typically test the latest 3 versions of LLVM to ensure that the eBPF program compiled by them
could be successfully loaded into the Linux kernels documented in [Linux Kernel Support Status](#linux-kernel-support-status).

| Version | Tested in CI | Status |
|:-------:|:------------:|:------:|
| 20      | ✅           | ✅     |
| 21      | ✅           | ✅     |
| 22      | ✅           | ✅     |

It is very likely that using other recent LLVM versions would work.
If you encounter bugs with an LLVM version that is not covered in our CI,
please [open an issue](https://github.com/kxxt/tracexec/issues) and we are happy to help out.