# Comparison with other tools

There are many existing tools for tracing exec syscalls when I start to create tracexec.
However, none of them suit my use case well.

This chapter provides a comparison between tracexec and those tools.
I hope this chapter will provide readers the knowledge about choosing the best tool for their
use cases.

We can roughly divide the tools into three categories by how exec tracing is implemented.
(Tracexec supports multiple ways for tracing exec)

| tool                | eBPF | Loadable Kernel Module | ptrace |
|---------------------|:----:|:----------------------:|:-------|
|tracexec             | ✅   | ❌                     | ✅     |
|strace               | ❌   | ❌                     | ✅     |
|execsnoop (bcc)      | ✅   | ❌                     | ❌     |
|execsnoop (bpftrace) | ✅   | ❌                     | ❌     |
|[execsnoop-nd.stp][1]| ❌   | ✅                     | ❌     |

[1]: https://github.com/brendangregg/systemtap-lwtools/blob/master/execsnoop-nd.stp
