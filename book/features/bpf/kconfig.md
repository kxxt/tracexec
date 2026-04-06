# Required Kernel Configs for eBPF Backend

## Required Config Entries

The eBPF backend of course needs a kernel with eBPF and ftrace enabled:

```perl
CONFIG_DEBUG_INFO_BTF=y
CONFIG_BPF_SYSCALL=y
CONFIG_BPF_EVENTS=y
CONFIG_FTRACE=y
CONFIG_FUNCTION_TRACER=y
CONFIG_KPROBES=y
CONFIG_KPROBE_EVENTS=y
```

We need the JIT of eBPF enabled and turned on because
when the JIT is disabled, the verifier rejects our program.

```perl
CONFIG_BPF_JIT=y
```

An optional but highly recommended config entry is:

```perl
CONFIG_FUNCTION_ERROR_INJECTION=y
```

It enables tracexec to use sleepable eBPF programs for tracing the entry of exec syscalls.
If this config is turned off, tracexec will use non-sleepable eBPF programs,
which should work fine for most cases but might fail to read some data from user-space when the data is not yet loaded into the RAM. This problem is thoroughly explained in a blog post: <https://mozillazg.com/2024/03/ebpf-tracepoint-syscalls-sys-enter-execve-can-not-get-filename-argv-values-case-en.html>.


## Example Config

[The config used in our UKCI](https://github.com/kxxt/tracexec/blob/main/nix/kernel-source.nix)
could serve as a reference for building a custom kernel that supports tracexec.
It is written in Nix. To obtain a raw kernel config, build the `.#ukci` target and then dig `/nix/store/*linux-config*` out of the nix store.
