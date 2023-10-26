# Changelog

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
