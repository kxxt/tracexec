# Changelog

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
- feat: --print-children for  a message when a child is created
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
