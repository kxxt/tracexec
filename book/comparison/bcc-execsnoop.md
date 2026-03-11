# Comparison with execsnoop(bcc)

This article compares tracexec with [the latest commit][commit] of execsnoop
at the time of writing. Feel free to improve it if you found anything outdated.

[commit]: https://github.com/iovisor/bcc/commit/4578cd9daa88460b96e0af1295f17dfa26a3d011

There are two execsnoop implementations in bcc, [one implemented with Python][py-impl],
[another one implemented with libbpf][libbpf-impl].
Here we will compare with the Python implementation as it supports more features
at the time of writing.

## Shortcomings of execsnoop(bcc)

### Default Limits are Too Limited

By default, execsnoop can only trace up to 20 arguments per exec event,
which is too limited to trace complex compiler invocations by various build systems.
It can be raised using `--max-args` argument.

And execsnoop hardcodes a very low limit(`128`) for the length of each argument,
if any argument exceeds this limit, it is silently truncated, resulting in
wrong output without any notification to the user.

### Cannot Show ARGV\[0\]

execsnoop shows filename in the place of the first argument(`argv[0]`) and
discards the real `argv[0]`.
Most of the time this is not important because `argv[0]` is the filename or
the basename of the filename.

However, sometimes `argv[0]` and filename are different and this difference plays
an important role on how the program behaves. For example,
multi-call binaries like busybox can act as different commands depending on `argv[0]`.

### Cannot Show Environment Variables

Sometimes, environment variables play a vital role in program execution.
execsnoop doesn't show them at all.

### Cannot Copy-Paste-Execute

A handy feature of tracexec is to copy the shell escaped command line to clipboard,
which you can directly paste into another terminal and hit enter to execute it.

But as for execsnoop. It doesn't even quote the arguments by default,
making it hard to distinguish the boundary between arguments.
Even if `-q/--quote` is used, there is still a long way to copy-paste-execute
because it does not perform shell escaping.
Even if it performs shell-escaping in the future. Without the environment variables,
the command may also not work.

## Missing features in tracexec compared with execsnoop(bcc)

execsnoop supports tracing processes under a cgroups path and limit tracing to a specific
UID.


[py-impl]: https://github.com/iovisor/bcc/blob/master/tools/execsnoop.py
[libbpf-impl]: https://github.com/iovisor/bcc/blob/master/libbpf-tools/execsnoop.c
