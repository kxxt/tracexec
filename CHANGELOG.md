# Changelog

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
