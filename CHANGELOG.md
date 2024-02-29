# Changelog

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
