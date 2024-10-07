# Changelog

## v0.6.2-rc.2

- Fix: Update dependencies to get rid of yanked futures-util 0.3.30
- Fix: Ensure build-script is built with the same vendoring feature as the main binary
- CI: Fix a typo in CI yaml that caused the released static binaries to be non static.
- CI: Enable eBPF for riscv64.
- CI: Enable static builds for riscv64.

## v0.6.1

- Docs: document dependencies for building in INSTALL.md.
- CI: Bump ubuntu to 24.04, with clang 18 as default clang.
- Previously, when building tracexec, even if `--no-default-feaures` is specified,
libbpf still gets vendored once because it is also depended in `libbpf-cargo` build dependency.
This release fixes it.
- Fix the help entry of breakpoint manager.
- Fix: eBPF: only remove pgid from closure if follow-forks
- Fix: eBPF: simplify program to make it load on kernel >= 6.8
- Fix: eBPF: add a temporary workaround(d7f23b4b66f9846cb3ae4d73ee60b30741092516) to make it load in release mode on new kernels.
A side effect is some empty printk output in `/sys/kernel/debug/tracing/trace_pipe`. See the commit for more details.

## v0.6.0

I am happy to announce that v0.6 brings the exciting eBPF backend🎉🎉🎉!

The eBPF backend supports system-wide exec tracing as well as good old follow-forks behavior.
It is still considered experimental but feel free to try it out! It should work on 6.x kernels.

Changes since v0.5.2:

### Installation

- The installation doc has been moved to `INSTALL.md`.
- Statically linked musl builds are no longer available due to `libbpf-sys` fails to compile with musl.
  - As an alternative, statically linked glibc builds are now available.
- New feature flags:
  - `recommended`: This enables the recommended functionalities of tracexec
  - `ebpf`: eBPF backend that doesn't use ptrace and could be used for system wide tracing
  - `ebpf-debug`: Not meant for end users. This flag enables debug logging to `/sys/kernel/debug/tracing/trace_pipe` and some debug checks.
  - `static`: Statically link libelf, zlib and libbpf.
  - `vendored`: Vendoring libelf, zlib and libbpf, implies `static`.
  - `vendored-libbpf`: Vendoring libbpf and statically link to it.

By default, we enable the `recommended` and `vendored-libbpf` features. This means that we are dynamically linking zlib and libelf but statically linking libbpf. This choice is made because zlib and libelf are usually installed on most systems but libbpf is usually not.

To dynamically link to libbpf, turn off default features and enable `recommended` feature:

### Breaking Changes

- Build with musl is no longer supported.
- Additional dependencies are required to build tracexec.
- The config file format should be updated.
  - `default_external_command` is moved to `debugger` section.
  - `seccomp_bpf` is moved to `ptrace` section.
  - `modifier` config section now also applies to eBPF backend.
  - `tui`, `log` config section now also apply to corresponding commands of eBPF backend.

### Added

- Add riscv64 support to seccomp feature (Note: `seccompiler` still doesn't support riscv64 yet. This would require using a fork)
- Add experimental eBPF backend with `log`, `tui` and `collect` commands.

### Changed

- Update dependencies
- Internal refactor
- TUI: Performance improvement for details popup.

### Fixed

- For experimental fd in cmdline feature, use `<>` instead of `>` for added fds.
- TUI: don't show layout help item when there's only one pane
- TUI: fix crash caused by Rect mismatch, joshka/tui-widgets#33
- When comparing fds, we now compare the mount id and inode number instead of naively comparing the path.

## v0.5.2

Changes since v0.5.1:

Show error when tracer thread crashed(e.g. when the command doesn't exist). Previously it hangs when tracer thread crashes.

Starting with this version, the tags are signed with my gpg key. The public key can be found here: http://keyserver.ubuntu.com:11371/pks/lookup?search=17AADD6726DDC58B8EE5881757670CCFA42CCF0A&fingerprint=on&op=index

## v0.5.1

Changes since v0.5.0:

Fix an incorrectly placed `continue` statement that causes tracee to hang when SIGALRM is sent to tracee.

## v0.5.0

Changes since v0.4.1:

### Features

The exec events can now be collected and saved as JSON stream or JSON format!
This feature is implemented by the new `collect` subcommand.

The JSON stream format is newline-delimited JSONs and when `--pretty`(which prettifies the JSON) is not enabled,
it is also a [JSON Lines text file](https://jsonlines.org/).
The first JSON of the JSON stream contains metadata like tracexec version and baseline environment information.
Other JSONs are exec events.

The JSON format is a big JSON object that contains metadata and an array of exec events in the `events` field.

And, tracexec now supports user-level profile🎉!

The profile file is a toml file that can be used to set fallback options.
It should be placed at `$XDG_CONFIG_HOME/tracexec/` or `$HOME/.config/tracexec/` and named `config.toml`.

A template profile file can be found at https://github.com/kxxt/tracexec/blob/main/config.toml

Note that the profile format is not stable yet and may change in the future. You may need to update your profile file when upgrading tracexec.

### Other changes

- Add `--profile` and `--no-profile` to load non-default profile and ignore profile, respectively.
- Update dependencies.
- Internal: Add a ruby script to update README.
- Internal: Some refactor work.

## v0.4.1

Changes since v0.4.0:

- Update dependencies, notably:
  - `rataui` to v0.27.0, and its friend crates
  - `shell-quote` to v0.7.1. The escape of utf8 characters is now better.
  - chore: run cargo update to get rid of yanked bytes 1.6.0
- Perf: Log Mode: Don't accumulate msgs on unbounded channel
- Docs: Update crate description

## v0.4.0

I am very excited to share that tracexec can now be used as a debugger launcher.

It's usually not trivial or convenient to debug a program executed by a shell/python script(which can use pipes as stdio for the program).
The following video shows how to use tracexec to launch gdb to detach two simple programs piped together by a shell script.

https://github.com/kxxt/tracexec/assets/18085551/72c755a5-0f2f-4bf9-beb9-98c8d6b5e5fd

Solves:

- https://stackoverflow.com/questions/5048112/use-gdb-to-debug-a-c-program-called-from-a-shell-script
- https://stackoverflow.com/questions/1456253/gdb-debugging-with-pipe
- https://stackoverflow.com/questions/455544/how-to-load-program-reading-stdin-and-taking-parameters-in-gdb
- https://stackoverflow.com/questions/65936457/debugging-a-specific-subprocess


To learn more about it, [read the gdb-launcher example](https://github.com/kxxt/tracexec/blob/main/demonstration/gdb-launcher/README.md).

Changes since v0.3.1:

### Added

- Breakpoints.
  - The breakpoints can be set in CLI(`--add-breakpoint/-b`) and TUI.
- Managing breakpoint hits.
  - in CLI: option `--default-external-command`
  - in TUI: Hit Manager
  - Detach, Resume, or Detach, stop and run external command
- `--tracer-delay` option for setting the polling delay of the tracer, in microseconds. The default is 500 when seccomp-bpf is enabled, otherwise 1.

### Changed

- Docs: make the description of --seccomp-bpf more clear

## v0.3.1

tracexec v0.3.1 released!

Changes since v0.3.0:

### Fixed

- TUI: Fix a bug that the event list is not refreshed when new events are available in some cases.

## v0.3.0

tracexec v0.3.0 released!

Changes since v0.2.2:

### Added

- Shell completions are now available for bash, elvish, fish, powershell and zsh!
  - Run `tracexec generate-completions <SHELL>` to get the completion file to install for your favorite shell.
  - Or generate completions when packaging tracexec so that users don't need to install the completions themselves.
- TUI: Toggle showing/hiding CWDS by pressing `W`.
- Musl builds are now available for x86_64 and aarch64.
- TUI: Add `Ctrl+U` key binding to bottom help text, which clears the text in the search bar when editing it.

### Changed

- TUI: To optimize memory usage(avoiding storing a contiguous string separately),
the internal regex implementation is switched to `regex-cursor` from `regex`.
- TUI: The order of the key bindings in the bottom help text is changed.

### Fixed

- Fix build issues on musl.
- TUI: Fix search result not being updated after toggling show/hide CWD/Env.
- TUI: Stop following when navigating through the search results.
- TUI: Fix incorrect wrapping behavior of the bottom key binding help text by updating rataui and use NBSPs.
- TUI: Fix crash when resizing the terminal by updating rataui.

### Performance

- Store more information as cached arcstr to reduce memory usage.
- Other optimizations to reduce memory usage.

## v0.3.0-alpha.1

tracexec v0.3.0-alpha.1 released!

Changes since v0.2.2:

### Added

- Shell completions are now available for bash, elvish, fish, powershell and zsh!
  - Run `tracexec generate-completions <SHELL>` to get the completion file to install for your favorite shell.
- TUI: Toggle showing/hiding CWDS by pressing `W`.
- Musl builds are now available for x86_64 and aarch64.

### Fixed

- Fix build issues on musl.

## v0.2.2

tracexec v0.2.2 released!

Changes since v0.2.1:

### Fixed

- Fix a race condition in the communication between the tracer and the TUI.
- TUI: Change the modifier key that toggles case sensitivity and regex/plain text in the search bar from `Ctrl` to `Alt`
because in most terminals, `Ctrl`+`I` is equivalent to `Tab` thus the toggle is not working as expected.
- Clarify that the license is `GPL-2.0-or-later` in Cargo.toml(was `GPL-2.0`).

### Performance

- Keep a global cache of env keys/values to reduce memory usage.

### Other

- Mark tests that need to be run single-threaded with `serial_test` crate so that we don't need to set `RUST_TEST_THREADS=1` when running tests.

## v0.2.1

tracexec v0.2.1 released!

Changes since v0.2.0:

- TUI: Fix a bug that when switching to follow mode, the event list is not scrolled to the bottom immediately.

## v0.2.0

tracexec v0.2.0 released!

![0.2.0](https://github.com/kxxt/tracexec/blob/main/screenshots/status.png?raw=true)

Changes since v0.1.0:

### Added

- TUI: The events can now be searched from a search bar(`Ctrl+F`).
  - Both case-sensitive and case-insensitive(default) are supported.
  - Both plain text(default) and regex search are supported.
- TUI: Show status icons for events.
- TUI: Show process status for exec events in details popup.
- TUI: More help text in the help dialog (`F1`).

### Changed

- Tracer: Automatically resolve `/proc/self/exe` symlink filename. (Use `--no-resolve-proc-self-exe` to disable)
- Log Mode: Control whether to set terminal foreground process group with `--foreground/--no-foreground`.
- TUI: don't show terminal cursor when terminal is not focused.
- Tweak log levels.

### Fixed

- Tracer: handle pid reuse correctly.
- TUI: Correctly handle unicode in the event list.
- TUI: Don't crash when inputting some control codes into the pseudo terminal(e.g. `Ctrl+4`).
- Log Mode: print new child with green pid.
- Don't set terminal foreground process group in tests.
- Add missing help text for `--no-decode-errno`.
- Fix CI for publishing to crates.io (excluding /sceeenshots from the package because it's too large)

## v0.2.0-rc.0

tracexec v0.2.0-rc.0 released!

![tracexec v0.1.0](https://github.com/kxxt/tracexec/blob/main/screenshots/status.png?raw=true)

Changes since v0.1.0:

### Added

- TUI: The events can now be searched from a search bar(`Ctrl+F`).
  - Both case-sensitive and case-insensitive(default) are supported.
  - Both plain text(default) and regex search are supported.
- TUI: Show status icons for events.


### Changed

- Tracer: Automatically resolve `/proc/self/exe` symlink filename. (Use `--no-resolve-proc-self-exe` to disable)
- Log Mode: Control whether to set terminal foreground process group with `--foreground/--no-foreground`.
- TUI: don't show terminal cursor when terminal is not focused.
- Tweak log levels.

### Fixed

- Tracer: handle pid reuse correctly.
- TUI: Correctly handle unicode in the event list.
- TUI: Don't crash when inputting some control codes into the pseudo terminal(e.g. `Ctrl+4`).
- Log Mode: print new child with green pid.
- Don't set terminal foreground process group in tests.
- Add missing help text for `--no-decode-errno`.
- Fix CI for publishing to crates.io (excluding /sceeenshots from the package because it's too large)

## v0.1.0

tracexec v0.1.0 is now finally released! 🎉🎉🎉

![tracexec v0.1.0](https://github.com/kxxt/tracexec/blob/main/screenshots/tui-demo.gif?raw=true)

This release includes TUI feature and many improvements and bug fixes.

### Notable changes since v0.0.5

#### Added

- An awesome TUI built with awesome ratatui.
- Tracing and diffing file descriptors to easily catch fd leaks and figure out inherited fds.
  - [Experimental] Try to construct cmdline that reproduces the same fds/stdio. (`--stdio-in-cmdline/--fd-in-cmdline`)
- Add `--user` option to run as a different user.  (This is mostly useful for tracing setuid/setgid binaries. Thanks to strace for the idea.)
  - Automatically disable seccomp-bpf when using `--user` because seccomp-bpf enforces no-new-privs.
- Add `-C` option to change the working directory of tracexec.
- Add `--filter{,-include,-exclude}` and `--show-all-events` option to filter events.
- Warn on bad envp/argv/filename and empty argv.
- TUI: Now tracexec can be themed at compile-time by changing `src/tui/theme.rs`.

#### Fixed

- Fix hang when root child is stopped by other signals before ptrace is setup
- Log mode: The formatting of interpreters now correctly respects color settings(e.g. NO_COLOR).
- Fix the logic of argv[0] handling for both logging and TUI mode.
- Log mode: Don't crash if `tcsetpgrp` returns `ENOTTY`.
- Some typos.

#### Changed

- Use `BTreeMap` to make environment variables sorted and deterministic.
- Internal logs are now logged to `$XDG_DATA_HOME/tracexec/tracexec.log`.
- Tracer thread now is named `tracer`.
- Some colors are changed in log mode.
- `--verbose/--quiet` is removed from CLI. Use `--filter{,-include,-exclude}` and `--show-all-events` instead.
- Log mode: `--show-cmdline` no longer implies `--successful-only`.

#### Other

- Add a few tests and CI.
- Enable LTO for release builds.
- Use `opt-level=1` for debug builds.

### Changes since v0.1.0-rc.1

- TUI: improve the message of tracee spawn/exit.
- TUI: don't omit tracee spawn event.

## v0.1.0-rc.1

tracexec v0.1.0-rc.1 released!

![tracexec v0.1.0-rc.1](https://github.com/kxxt/tracexec/blob/main/screenshots/tui-demo.gif?raw=true)

Changes since v0.1.0-rc.0:

### Added

- Enable LTO for release builds.
- TUI: Handle F1-F12 keys and Alt+key in pseudo terminal.
- TUI: Now tracexec can be themed at compile-time by changing `src/tui/theme.rs`.

### Changed

- Set max tracing level to info for release builds.
- Remove `log` dependency.
- Use `opt-level=1` for debug builds.
- Documentation update.
- Log: disable diff-fd by default when stdio-in-cmdline is enabled.

### Fixed

- Fix some typos.
- TUI: Don't handle key event when there are modifiers but shouldn't.
- docs: update install command for `cargo install` to avoid installing fixtures.
- Don't show `O_CLOEXEC` fds in cmdline.

## v0.1.0-rc.0

tracexec v0.1.0-rc.0 released!

### Added

- TUI: toggle showing/hiding the environment variables by pressing `E`.
- CI: initialize Continuous Integration with GitHub Actions.
- CI: setup cargo-deny and cargo-about.

### Fixed

- TUI: don't select past the last event.
- TUI: don't display header before cmdline in details popup.
- Don't set `SHELL` if it is not present in the environment.
- Test: add more details for assertion failures.

### Changed

- Use `BTreeMap` to make environment variables sorted and deterministic.
- TUI: show fd at last to make argv more visible.
- TUI: pty pane's title is now "Terminal" instead of "Pseudo Terminal".
- docs: update README for 0.1.0

## v0.1.0-beta.3

tracexec v0.1.0-beta.3 released!

This should be the last beta release before v0.1.0. All the features I want in v0.1.0 are already implemented.
I am starting to add some tests and looking for bugs to fix.

Changes since v0.1.0-beta.2:

### Added

- TUI: Display file descriptor flags in the FdInfo tab of the details popup.

### Fixed

- Don't crash if `tcsetpgrp` returns `ENOTTY`
- It's now documented that `--color` has no effect on TUI.
- Some typos.

### Changed

- TUI: Copy popup now has a green border.

## v0.1.0-beta.2

tracexec v0.1.0-beta.2 released!

![tracexec v0.1.0-beta.2](https://github.com/kxxt/tracexec/blob/main/screenshots/0.1.0-beta.2.gif?raw=true)

Changes since v0.1.0-beta.1:

### Added

- Tracing and diffing file descriptors.
- Option to show stdio/fds in cmdline.
- TUI: show detailed information of file descriptors in the FdInfo tab of details popup.

### Changed

- Update dependencies.
- TUI: Make CLI flags in help dialog more readable.
- Warn if argv is empty.
- Warn on bad envp/argv/filename.
- Log: `--show-cmdline` no longer implies `--successful-only`
- Warnings are now shown in TUI/Log mode.
- `--verbose/--quiet` is removed from CLI. Use `--filter/--filter-include/--filter-exclude` instead.

### Fixed

- Don't crash when tracee closes its stdio.
- TUI: fix truncated tabs.

## v0.1.0-beta.1

tracexec v0.1.0-beta.1 released!

Changes since v0.1.0-alpha.8:

### Added

- Add "Environment" tab to the details popup in TUI.
- Add scroll bars to event list in TUI.
- Handle argv[0] in logging mode.
- Send `Ctrl+S` to pty by pressing `Alt+S` when event list is active in TUI.

### Changed

- TUI now automatically selects the first/last event when the list is scrolled to the top/bottom or page up/down.
- In logging mode, the color of pid now matches TUI.

### Fixed

- Don't use option separator `-` in cmdline because it implies `--ignore-environment`.
- Fix the logic of argv[0] handling for both logging and TUI mode.
- Handle edge cases for the TUI event list when there are no events.
- Two off-by-one errors in the TUI event list.
- Clean up legacy code in pseudo terminal handling.
- Some typos.

## v0.1.0-alpha.8

tracexec v0.1.0-alpha.8 released!

Changes since v0.1.0-alpha.7:

### Added/Changed

- TUI: show basic statistics of events
- TUI: change colors for exec results.
- TUI: set frame rate from CLI by `--frame-rate/-F` option.
- TUI: default frame rate is now 60(previously 30).
- TUI: Add more details and scrollbar to the details popup.
- TUI: Copy to clipboard now works for the details popup.

### Optimizations

- Tweak tokio worker thread count.
- Reduce idle CPU usage in TUI mode.
  - Lines and List are now cached for the event list.

### Fixed

- The formatting of interpreters now correctly respects color settings(e.g. NO_COLOR).

## v0.1.0-alpha.7

tracexec v0.1.0-alpha.7 released!

Changes since v0.1.0-alpha.6:

### Added/Changed

- TUI: A basic details view is added.
- TUI: Copy to clipboard feature is added.
- TUI: Press any key to close the help dialog.
- Internal refactor and optimization.

## v0.1.0-alpha.6

tracexec v0.1.0-alpha.6 released!

![tracexec v0.1.0-alpha.6](https://github.com/kxxt/tracexec/blob/main/screenshots/0.1.0-alpha.6.png?raw=true)

Changes since v0.1.0-alpha.5:

### Added/Changed

- The panes in the TUI can now be resized by `G` and `S` keys.
- Vertical layout for the TUI is now supported. Use `--layout vertical` to enable it.
  (Or dynamically switch between horizontal and vertical layout by `Alt+L` in the TUI)
- Line wrapping for bottom help text in the TUI.
- Hide navigation key bindings from the bottom help text in the TUI.
- Show verbose help text in the TUI when pressing `F1`.
- In TUI, failed exec events with `ENOENT` are now given a special color.
- Update the style of selected items and arg0 for the TUI.
- Title now shows on the left top corner in the TUI (alongside version).
- Scroll to (start/end)/top/bottom in the TUI by `(Shift + ) Home/End` keys.

### Fixed

- Don't render the TUI when the terminal is too small
- Don't horizontally scroll past content.

## v0.1.0-alpha.5

tracexec v0.1.0-alpha.5 released!

Changes since v0.1.0-alpha.4:

![tracexec v0.1.0-alpha.5](https://github.com/kxxt/tracexec/blob/main/screenshots/0.1.0-alpha.5.png?raw=true)

### Added

- Horizontal scrolling in the TUI
- Use `Ctrl+S` to switch active pane in the TUI
- Event filter option(--filter). (Meanwhile, the tracing args are dropped for TUI mode)
- Option to set default active pane for TUI in the command line
- PageUp/PageDown/PageLeft/PageRight to scroll faster in the TUI

### Changed

- Tracer thread now is named `tracer`.
- Optimization: only render the visible part of the events in the TUI.
- PTY master is now closed when TUI exits.
- TUI now shows the cmdline for exec events.

### Fixed

- Fix hang when root child is stopped by other signals before ptrace is setup
- Fix selection and resize for the event list in the TUI
- Fix that TUI doesn't display failed exec events
- Some typos

## v0.1.0-alpha.4

tracexec v0.1.0-alpha.4 released!

Changes since v0.1.0-alpha.3:

### New Features

- Added `-C` option to change the working directory of tracexec.
- Added terminate/kill on exit option to TUI command.
- Added `--user` option to run as a different user. (This is mostly useful for tracing setuid/setgid binaries. Thanks to strace for the idea.)
  - Automatically disable seccomp-bpf when using `--user` because seccomp-bpf enforces no-new-privs.

![tracexec tracing across setuid binaries](https://github.com/kxxt/tracexec/blob/6fac526/screenshots/trace-suid.png?raw=true)

### Fixes

- Fix wrong cwd used to spawn child processes. This bug was introduced when switching to use `CommandBuilder` in v0.1.0-alpha.3.
- Fix `RUST_LOG` env var getting overwritten by tracexec. tracexec should not touch the environment variables at all.

## v0.1.0-alpha.3

tracexec v0.1.0-alpha.3 released!

Changes since v0.0.5:

- Added experimental TUI command.
- Logs are no longer output to stderr, but saved to a file instead.
- Internal refactor.

## v0.0.5

tracexec v0.0.5 released!

Changes since v0.0.4:

- Seccomp-bpf optimization is implemented and enabled by default. This almost reduces the performance overhead of tracexec to zero.
  - `--seccomp-bpf` option is added to control this feature.
  - Added a warning when running on untested low kernel versions (<4.8).
- Bug fixes for `--no-show-env`.
- List is now highlighted when using `--more-colors`.

## v0.0.5-rc.1

Changes since v0.0.4:

- Seccomp-bpf optimization is implemented and enabled by default. This almost reduces the performance overhead of tracexec to zero.
  - `--seccomp-bpf` option is added to control this feature.
  - Added a warning when running on untested low kernel versions (<4.8).
- Bug fixes for `--no-show-env`.
- List is now highlighted when using `--more-colors`.

## v0.0.4

tracexec v0.0.4 released!

Changes since v0.0.3:

- `--show-cmdline` now always shows the filename in the place of argv[0]. A warning will be logged if the filename does not match argv[0].
- Log level is now controlled via `--verbose` and `--quiet` flags instead of `RUST_LOG` environment variable.

## v0.0.3

tracexec v0.0.3 released!

Changes since v0.0.2:

- Fix hangs in some cases because SIGCHILD is not delivered to tracee.

## v0.0.2

tracexec v0.0.2 released!

Changes since v0.0.1:

- Add riscv64 support
- Fix a bug that a equal sign incorrectly got printed in the printed cmdline.
- Change description.

## v0.0.2-rc.1

tracexec v0.0.2 released!

Changes since v0.0.1:

- Add riscv64 support
- Fix a bug that a equal sign incorrectly got printed in the printed cmdline

## v0.0.1

tracexec v0.0.1 released!

Changes since v0.0.0-experimental.7:

- feat: --output, stderr by default
- feat: set foreground process group
- feat: use exit code from root child
- cli: allow show-filename to be used with show-cmdline
- cli: rename some options
- docs: update README

## v0.0.0-experimental.7

- Fix github release workflow
- cli: add author, version, about and more help

## v0.0.0-experimental.6

- Internal refactor and bug fixes
- feat(cli): add color level option
- --print-cmdline: show cmdline hint
- more colors
- deps: update shell-quote to 0.3.2, which makes the output of `--print-cmdline` more aesthetically pleasing.
- feat: --print-children for a message when a child is created
- docs: update README.
- Github: add release workflow

## v0.0.0-experimental.5

- Make the output readable on most color profiles (Simply don't change the background.)
- Add aarch64 support.
- Fix code that previously relies on x86_64 specific behaviors.
- feat: also trace execveat

## v0.0.0-experimental.4

- fix: handle ESRCH in ptrace requests
- feat: diff env by default
- feat: print_cmdline option
- feat: trace shebang interpreter
- feat: even more colors!
- fix: don't show extra comma in diff-env output

## v0.0.0-experimental.3

- Warn on bad memory read on tracee.
- Workaround execveat quirk
- Remove indent feature.
- Make CLI trace args work.
- `diff-env` now works!
- We now have colors!

## v0.0.0-experimental.2

- Make children process handling more robust
- CLI: Add `indent` option
- CLI: Add `decode-errno` option
- CLI: Rename `graph` command to `tree` (still unimplemented)

## v0.0.0-experimental.1

- Initial release
