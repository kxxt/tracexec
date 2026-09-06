# tracexec

A small utility for tracing execve{,at} and pre-exec behavior.

tracexec helps you to figure out what and how programs get executed when you execute a command.

It's useful for debugging build systems, understanding what shell scripts actually do, figuring out what programs
does a proprietary software run, etc.

![One tracexec TUI view split diagonally between the built-in (A), Amber (B), and Nord (C) themes.](book/assets/tui-themes.png)

- [Installation Guide](INSTALL.md)
- [Showcases](https://tracexec.kxxt.dev/introduction/showcases.html)

## Usage

General CLI help:

```bash
Core crate of tracexec [Internal implementation! DO NOT DEPEND ON!]

Usage: tracexec [OPTIONS] <COMMAND>

Commands:
  log                   Run tracexec in logging mode
  tui                   Run tracexec in TUI mode with a pseudo terminal by default
  generate-completions  Generate shell completions for tracexec
  collect               Collect exec events and export them
  ebpf                  Experimental ebpf mode
  help                  Print this message or the help of the given subcommand(s)

Options:
      --color <COLOR>      Control whether colored output is enabled. This flag has no effect on TUI mode. [default: auto] [possible values: auto, always, never]
  -C, --cwd <CWD>          Change current directory to this path before doing anything
  -P, --profile <PROFILE>  Load profile from this path
      --no-profile         Do not load profiles
  -u, --user <USER>        Run as user. This option is only available when running tracexec as root
      --elevate            Re-execute tracexec with privilege elevation (e.g. via sudo). The original user credentials are saved and the tracee will run as the original user.
  -h, --help               Print help
  -V, --version            Print version

```

TUI Mode:

```bash
Run tracexec in TUI mode with a pseudo terminal by default

Usage: tracexec tui [OPTIONS] -- <CMD>...

Arguments:
  <CMD>...  command to be executed

Options:
      --successful-only
          Only show successful calls
      --fd-in-cmdline
          [Experimental] Try to reproduce file descriptors in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.
      --stdio-in-cmdline
          [Experimental] Try to reproduce stdio in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.
      --resolve-proc-self-exe
          Resolve /proc/self/exe symlink
      --no-resolve-proc-self-exe
          Do not resolve /proc/self/exe symlink
      --hide-cloexec-fds
          Hide CLOEXEC fds
      --no-hide-cloexec-fds
          Do not hide CLOEXEC fds
      --timestamp
          Show timestamp information
      --no-timestamp
          Do not show timestamp information
      --inline-timestamp-format <INLINE_TIMESTAMP_FORMAT>
          Set the format of inline timestamp. See https://docs.rs/chrono/latest/chrono/format/strftime/index.html for available options.
      --collect-cgroup
          Collect cgroup information
      --no-collect-cgroup
          Do not collect cgroup information
      --seccomp-bpf <SECCOMP_BPF>
          Controls whether to enable seccomp-bpf optimization, which greatly improves performance [default: auto] [possible values: auto, on, off]
      --polling-interval <POLLING_INTERVAL>
          Polling interval, in microseconds. -1(default) disables polling.
      --show-all-events
          Set the default filter to show all events. This option can be used in combination with --filter-exclude to exclude some unwanted events.
      --filter <FILTER>
          Set the default filter for events. [default: warning,error,exec,tracee-exit]
      --filter-include <FILTER_INCLUDE>
          Aside from the default filter, also include the events specified here. [default: <empty>]
      --filter-exclude <FILTER_EXCLUDE>
          Exclude the events specified here from the default filter. [default: <empty>]
      --no-tty
          Do not allocate a pseudo terminal; redirect stdin/out/err to /dev/null
  -f, --follow
          Keep the event list scrolled to the bottom
      --terminate-on-exit
          Instead of waiting for the root child to exit, terminate when the TUI exits
      --kill-on-exit
          Instead of waiting for the root child to exit, kill when the TUI exits
  -A, --active-pane <ACTIVE_PANE>
          Set the default active pane to use when TUI launches [possible values: terminal, events]
  -L, --layout <LAYOUT>
          Set the layout of the TUI when it launches [possible values: horizontal, vertical]
  -F, --frame-rate <FRAME_RATE>
          Set the frame rate of the TUI (60 by default)
  -m, --max-events <MAX_EVENTS>
          Max number of events to keep in TUI (0=unlimited)
      --scrollback-lines <SCROLLBACK_LINES>
          Number of scrollback lines to keep in the pseudo terminal (1000 by default)
      --theme <THEME_FILE>
          Path to a theme file to use for the TUI.
  -D, --default-external-command <DEFAULT_EXTERNAL_COMMAND>
          Set the default external command to run when using "Detach, Stop and Run Command" feature in Hit Manager
  -b, --add-breakpoint <BREAKPOINTS>
          Add a new breakpoint to the tracer. This option can be used multiple times. The format is <syscall-stop>:<pattern-type>:<pattern>, where syscall-stop can be sysenter or sysexit, pattern-type can be argv-regex, in-filename or exact-filename. For example, sysexit:in-filename:/bash
  -h, --help
          Print help

```

Log Mode:

```bash
Run tracexec in logging mode

Usage: tracexec log [OPTIONS] -- <CMD>...

Arguments:
  <CMD>...  command to be executed

Options:
      --more-colors
          More colors
      --less-colors
          Less colors
      --show-cmdline
          Print commandline that (hopefully) reproduces what was executed. Note: file descriptors are not handled for now.
      --no-show-cmdline
          Don't print commandline that (hopefully) reproduces what was executed.
      --show-interpreter
          Try to show script interpreter indicated by shebang
      --no-show-interpreter
          Do not show script interpreter indicated by shebang
      --foreground
          Set the terminal foreground process group to tracee. This option is useful when tracexec is used interactively. [default]
      --no-foreground
          Do not set the terminal foreground process group to tracee
      --diff-fd
          Diff file descriptors with the original std{in/out/err}
      --no-diff-fd
          Do not diff file descriptors
      --show-fd
          Show file descriptors
      --no-show-fd
          Do not show file descriptors
      --diff-env
          Diff environment variables with the original environment
      --no-diff-env
          Do not diff environment variables
      --show-env
          Show environment variables
      --no-show-env
          Do not show environment variables
      --show-comm
          Show comm
      --no-show-comm
          Do not show comm
      --show-argv
          Show argv
      --no-show-argv
          Do not show argv
      --show-filename
          Show filename
      --no-show-filename
          Do not show filename
      --show-cwd
          Show cwd
      --no-show-cwd
          Do not show cwd
      --decode-errno
          Decode errno values
      --no-decode-errno
          Do not decode errno values
      --successful-only
          Only show successful calls
      --fd-in-cmdline
          [Experimental] Try to reproduce file descriptors in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.
      --stdio-in-cmdline
          [Experimental] Try to reproduce stdio in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.
      --resolve-proc-self-exe
          Resolve /proc/self/exe symlink
      --no-resolve-proc-self-exe
          Do not resolve /proc/self/exe symlink
      --hide-cloexec-fds
          Hide CLOEXEC fds
      --no-hide-cloexec-fds
          Do not hide CLOEXEC fds
      --timestamp
          Show timestamp information
      --no-timestamp
          Do not show timestamp information
      --inline-timestamp-format <INLINE_TIMESTAMP_FORMAT>
          Set the format of inline timestamp. See https://docs.rs/chrono/latest/chrono/format/strftime/index.html for available options.
      --collect-cgroup
          Collect cgroup information
      --no-collect-cgroup
          Do not collect cgroup information
      --seccomp-bpf <SECCOMP_BPF>
          Controls whether to enable seccomp-bpf optimization, which greatly improves performance [default: auto] [possible values: auto, on, off]
      --polling-interval <POLLING_INTERVAL>
          Polling interval, in microseconds. -1(default) disables polling.
      --show-all-events
          Set the default filter to show all events. This option can be used in combination with --filter-exclude to exclude some unwanted events.
      --filter <FILTER>
          Set the default filter for events. [default: warning,error,exec,tracee-exit]
      --filter-include <FILTER_INCLUDE>
          Aside from the default filter, also include the events specified here. [default: <empty>]
      --filter-exclude <FILTER_EXCLUDE>
          Exclude the events specified here from the default filter. [default: <empty>]
  -o, --output <OUTPUT>
          Output, stderr by default. A single hyphen '-' represents stdout.
  -h, --help
          Print help

```

Collect and export data:

```
Collect exec events and export them

Usage: tracexec collect [OPTIONS] --format <FORMAT> -- <CMD>...

Arguments:
  <CMD>...  command to be executed

Options:
      --successful-only
          Only show successful calls
      --fd-in-cmdline
          [Experimental] Try to reproduce file descriptors in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.
      --stdio-in-cmdline
          [Experimental] Try to reproduce stdio in commandline. This might result in an unexecutable cmdline if pipes, sockets, etc. are involved.
      --resolve-proc-self-exe
          Resolve /proc/self/exe symlink
      --no-resolve-proc-self-exe
          Do not resolve /proc/self/exe symlink
      --hide-cloexec-fds
          Hide CLOEXEC fds
      --no-hide-cloexec-fds
          Do not hide CLOEXEC fds
      --timestamp
          Show timestamp information
      --no-timestamp
          Do not show timestamp information
      --inline-timestamp-format <INLINE_TIMESTAMP_FORMAT>
          Set the format of inline timestamp. See https://docs.rs/chrono/latest/chrono/format/strftime/index.html for available options.
      --collect-cgroup
          Collect cgroup information
      --no-collect-cgroup
          Do not collect cgroup information
      --seccomp-bpf <SECCOMP_BPF>
          Controls whether to enable seccomp-bpf optimization, which greatly improves performance [default: auto] [possible values: auto, on, off]
      --polling-interval <POLLING_INTERVAL>
          Polling interval, in microseconds. -1(default) disables polling.
  -p, --pretty
          prettify the output if supported
  -F, --format <FORMAT>
          the format for exported exec events [possible values: json-stream, json, perfetto]
  -o, --output <OUTPUT>
          Output, stderr by default. A single hyphen '-' represents stdout.
      --foreground
          Set the terminal foreground process group to tracee. This option is useful when tracexec is used interactively. [default]
      --no-foreground
          Do not set the terminal foreground process group to tracee
  -h, --help
          Print help

```

eBPF backend supports similar commands:

```
Experimental ebpf mode

Usage: tracexec ebpf <COMMAND>

Commands:
  log      Run tracexec in logging mode
  tui      Run tracexec in TUI mode, with a pseudo terminal when following a command
  collect  Collect exec events and export them
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

```

## Profile

`tracexec` can be configured with a profile file. The profile file is a toml file that can be used to set fallback options.

The profile file should be placed at `$XDG_CONFIG_HOME/tracexec/` or `$HOME/.config/tracexec/` and named `config.toml`.

A template profile file can be found at https://github.com/kxxt/tracexec/blob/main/config.toml

As a warning, the profile format is not stable yet and may change in the future. You may need to update your profile file when upgrading tracexec.

TUI themes can be configured in the `tui` section of the profile in one of two ways:

```toml
[tui]
theme-file = "nord.toml"
```

`theme-file` accepts either an absolute path or a path relative to the theme directories. Relative paths are resolved relative to the following absolute paths in order:

1. `$XDG_CONFIG_HOME/tracexec/themes/` (or `$HOME/.config/tracexec/themes/`)
2. `$XDG_DATA_HOME/tracexec/themes/` (or `$HOME/.local/share/tracexec/themes/`)
3. `/etc/tracexec/themes`
4. `<path_to_tracexec_binary>/../share/tracexec/themes/`

You can also inline the theme definition directly in the profile. Theme entries patch the built-in default theme, so you only need to specify the parts you want to change.

```toml
[tui]
theme = { app-title = { fg = "cyan" }, active-border = { fg = "light-cyan" } }
```

Example themes are available in the repository's `themes/` directory.

## Known issues

- Non UTF-8 strings are converted to UTF-8 in a lossy way, which means that the output may be inaccurate.
- The output is not stable yet, which means that the output may change in the future.
- The pseudo terminal can't pass through certain key combinations and terminal features.

## Origin

This project was born out of the need to trace the execution of programs.

Initially I simply use `strace -Y -f -qqq -s99999 -e trace=execve,execveat <command>`.

But the output is still too verbose so that's why I created this project.

## Credits

This project takes inspiration from [strace](https://strace.io/) and [lurk](https://github.com/JakWai01/lurk).
