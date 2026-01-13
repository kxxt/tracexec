# Contributing

Thank you for considering contributing to tracexec!
You can contribute to tracexec by submitting bug reports or pull requests.

## Bug Reports

Before opening a bug report, please search for similar issues first to avoid creating duplicates.

When opening a bug report, please
- include the version number of tracexec that has the bug, e.g. v0.16.0,
- include the operating system, CPU architecture and kernel version, e.g. CachyOS, x86_64, `6.18.4-2-cachyos`,
- indicate where you obtained the tracexec binary (e.g. GitHub Releases, Packaging Repo of Linux Distributions),

## Pull Requests

Before you start to work on a new feature for tracexec,
please file a feature request to better discuss it to avoid duplicated efforts or spending time on features that may not
be accepted into tracexec.

For adding a new backend to trace exec events, you need to justify why existing backend could not satisfy your requirement
and you may be asked to maintain the new backend depending on the complexity of it.

For adding new architecture support, you should agree to be mentioned in GitHub issues or discussions to provide feedbacks when
other users open a bug report related to your added architecture support.

For adding a new exporter, you may be asked to maintain the new exporter depending on the complexity of it.

### Check List

- `cargo clippy --workspace` should pass.
- `cargo +nightly fmt` before commit.
- `cargo test --workspace` should pass.
- You are welcome to contribute unit tests for new features.
