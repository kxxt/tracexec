# Comparison with strace

[strace] is a generic syscall tracing tool that could be used for tracing exec.
This article will compare tracexec with the latest version (6.19) of [strace]
at the time of writing. Feel free to improve it if you found anything outdated.

## Shortcomings of strace

### Missing a Sane Verbosity Level

To trace exec, the most simple strace command that comes to my mind is:

#### Default Verbosity

```bash
strace -e trace=execveat,execve -f -- bash
```

This produces a noisy log with lots of unrelated content that makes it hard to find the exec events:

```c
[pid 522056] +++ exited with 0 +++
[pid 522051] --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=522056, si_uid=1000, si_status=0, si_utime=0, si_stime=0} ---
[pid 522055] +++ exited with 0 +++
[pid 522051] --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=522055, si_uid=1000, si_status=0, si_utime=0, si_stime=0} ---
[pid 522059] +++ exited with 0 +++
[pid 522058] +++ exited with 0 +++
strace: Process 522061 attached
strace: Process 522062 attached
strace: Process 522063 attached
strace: Process 522064 attached
```

While for actual exec events, it is not verbose because the environment variables are hidden.

```c
[pid 522055] execve("/usr/bin/ip", ["/usr/bin/ip", "netns", "identify"], 0x7ffe989a3dc0 /* 110 vars */ <unfinished ...>
[Five lines omitted]
[pid 522055] <... execve resumed>)      = 0
```

#### Quiet

If we use `-q/--quiet`, that still does not fix the noisy log problem.

```bash
strace -e trace=execveat,execve -f -q -- bash
```

But at least it makes logs like `strace: Process 522061 attached` go away.

#### Verbose

What if we want to know the environment variables? We need to increase verbosity:

```bash
strace -e trace=execveat,execve -f -q -- bash
```

We still have a very noisy log but we could see the environment variables. (Well, at least the env variable names...)

`[pid 571872] execve("/usr/bin/ip", ["/usr/bin/ip", "netns", "identify"], ["SHELL=/usr/bin/zsh", "SESSION_MANAGER=local/ryzen:@/tm"..., "USER_ZDOTDIR=/home/kxxt", "COLORTERM=truecolor", "XDG_CONFIG_DIRS=/home/kxxt/.conf"..., "VSCODE_DEBUGPY_ADAPTER_ENDPOINTS"..., "XDG_SESSION_PATH=/org/freedeskto"..., "XDG_MENU_PREFIX=plasma-", "TERM_PROGRAM_VERSION=1.112.01907", "ICEAUTHORITY=/run/user/1000/icea"..., "LC_ADDRESS=en_US.UTF-8", "USE_CCACHE=1", "LC_NAME=en_US.UTF-8", "SSH_AUTH_SOCK=/run/user/1000/gnu"..., "MEMORY_PRESSURE_WRITE=c29tZSAyMD"..., "PYDEVD_DISABLE_FILE_VALIDATION=1", "DESKTOP_SESSION=plasma", "LC_MONETARY=en_US.UTF-8", "__ETC_PROFILE_NIX_SOURCED=1", "GTK_RC_FILES=/etc/gtk/gtkrc:/hom"..., "NO_AT_BRIDGE=1", "EDITOR=nvim", "XDG_SEAT=seat0", "PWD=/home/kxxt/repos/tracexec", "NIX_PROFILES=/nix/var/nix/profil"..., "LOGNAME=kxxt", "XDG_SESSION_DESKTOP=KDE", "XDG_SESSION_TYPE=wayland", "SYSTEMD_EXEC_PID=2534", "BUNDLED_DEBUGPY_PATH=/home/kxxt/"..., "XAUTHORITY=/run/user/1000/xauth_"..., "VSCODE_GIT_ASKPASS_NODE=/usr/sha"..., "MOTD_SHOWN=pam", "VSCODE_INJECTION=1", "GTK2_RC_FILES=/etc/gtk-2.0/gtkrc"..., "HOME=/home/kxxt", "MCFLY_HISTORY=/tmp/mcfly.wGDRsBB"..., "SSH_ASKPASS=/run/user/1000/gnupg"..., "MCFLY_FUZZY=true", "LANG=en_US.UTF-8", "LC_PAPER=en_US.UTF-8", "MCFLY_HISTFILE=/home/kxxt/.zhist"..., "_JAVA_AWT_WM_NONREPARENTING=1", "XDG_CURRENT_DESKTOP=KDE", "PYTHONSTARTUP=/home/kxxt/.config"..., "MEMORY_PRESSURE_WATCH=/sys/fs/cg"..., "STARSHIP_SHELL=bash", "WAYLAND_DISPLAY=wayland-0", "__MISE_DIFF=eAFrXpyfk9KwOC+1vGFJ"..., "NIX_SSL_CERT_FILE=/etc/ssl/certs"..., "GIT_ASKPASS=/usr/share/vscodium/"..., "XDG_SEAT_PATH=/org/freedesktop/D"..., "INVOCATION_ID=3175cd2e73284f8aab"..., "MANAGERPID=2130", "MCFLY_SESSION_ID=ONlRzDF6foVRPQ7"..., "CHROME_DESKTOP=codium.desktop", "STARSHIP_SESSION_KEY=82575444194"..., "__MISE_ORIG_PATH=/home/kxxt/.car"..., "KDE_SESSION_UID=1000", "VSCODE_GIT_ASKPASS_EXTRA_ARGS=", "VSCODE_PYTHON_AUTOACTIVATE_GUARD"..., "XDG_SESSION_CLASS=user", "ANDROID_HOME=/opt/android-sdk", "TERM=xterm-256color", "LC_IDENTIFICATION=en_US.UTF-8", "PYTHON_BASIC_REPL=1", "__MISE_ZSH_PRECMD_RUN=1", "MCFLY_RESULTS_SORT=LAST_RUN", "ZDOTDIR=/home/kxxt", "USER=kxxt", "VSCODE_GIT_IPC_HANDLE=/run/user/"..., "CUDA_PATH=/opt/cuda", "QT_WAYLAND_RECONNECT=1", "KDE_SESSION_VERSION=6", "PAM_KWALLET5_LOGIN=/run/user/100"..., "__MISE_SESSION=eAHqWpOTn5iSmhJfk"..., "MCFLY_HISTORY_FORMAT=zsh", "MCFLY_RESULTS=20", "DISPLAY=:0", "SHLVL=3", "LC_TELEPHONE=en_US.UTF-8", "ANDROID_SDK_ROOT=/opt/android-sd"..., "CCACHE_EXEC=/usr/bin/ccache", "LC_MESSAGES=en_US.UTF-8", "LC_MEASUREMENT=en_US.UTF-8", "XDG_VTNR=2", "XDG_SESSION_ID=2", "MANAGERPIDFDID=2131", "CUDA_DISABLE_PERF_BOOST=1", "FC_FONTATIONS=1", "XDG_RUNTIME_DIR=/run/user/1000", "DEBUGINFOD_URLS=https://debuginf"..., "NVCC_CCBIN=/usr/bin/g++", "MCFLY_INTERFACE_VIEW=BOTTOM", "LC_TIME=en_US.UTF-8", "VSCODE_GIT_ASKPASS_MAIN=/usr/sha"..., "JOURNAL_STREAM=9:44333", "MISE_SHELL=bash", "XDG_DATA_DIRS=/home/kxxt/.local/"..., "GDK_BACKEND=wayland", "KDE_FULL_SESSION=true", "PATH=/home/kxxt/.local/share/mis"..., "DBUS_SESSION_BUS_ADDRESS=unix:pa"..., "KDE_APPLICATIONS_AS_SCOPE=1", "HG=/usr/bin/hg", "MAIL=/var/spool/mail/kxxt", "LC_NUMERIC=en_US.UTF-8", "OLDPWD=/home/kxxt/repos/tracexec", "TERM_PROGRAM=vscode", "_=/usr/bin/starship"]) = 0`


Many variables are truncated because it exceeds the string length limit.

#### Showing the Full Environment Variables

To show the full environment variables, increase the string length limit with `-s/--string-limit`:

```bash
strace -e trace=execveat,execve -f -v -s99999 -- bash
```

#### Finally Reaching A Sane Verbosity

To silence all other noisy logs while logging all environment variables, we could use:

```bash
strace -e trace=execveat,execve -vqqq -e 'signal=!all' -f -s99999 -- bash
```

But that command line has become too long to type and remember. With tracexec, it is much easier to remember:

```bash
tracexec log --show-env -- bash
```

### Cannot Diff Environment Variables

In the previous shortcoming, we can see that strace could show all the environment variables used in exec.
However, showing all the environment variables is too verbose. Most of the time we are only interested in
the diff of environment variables. Or to put it in another way, what environment are added and which are modified or removed.

strace has no support for doing that but tracexec by default shows diff of environment variables:

```bash
tracexec log -- bash
```

### Cannot Copy-Paste-Execute

A handy feature of tracexec is to copy the shell escaped command line to clipboard,
which you can directly paste into another terminal and hit enter to execute it.

But as for strace. It prints the arguments in an array syntax,
making it impossible to directly copy and paste into shell.

## Missing features in tracexec compared with strace

### Tracing only a single process

[strace] supports tracing only a single process when `-f/--follow-forks` is not enabled.

In tracexec, we think this use case is too narrow to fit into a specialized exec tracing tool
and didn't implement it.

### Stack trace

[strace] supports printing a stack trace at syscall with `-k`. We are working on supporting it
in tracexec: <https://github.com/kxxt/tracexec/issues/108>.

[strace]: https://strace.io
