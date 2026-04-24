# Tests

tracexec currently contains two kinds of tests:
- the normal tests that are executed when running `cargo test --workspace`,
- tests requiring root that are excluded by default.

## Running the Tests

To run the normal tests, use

```bash
cargo test --workspace
```

`sudo` is needed to run the tests that requires root:

```bash
CARGO_TARGET_<TARGET_TRIPLE>_RUNNER='sudo -E' cargo test --workspace -- --ignored
```

For example, if you are testing on a x86_64 linux machine, use

```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E' cargo test --workspace -- --ignored
```

## Test Coverage

Most of the time you do not need to calculate the test coverage by yourself because we are tracking
the test coverage continuously with [CodeCov](https://about.codecov.io/).

You will see the patch coverage and code coverage diff in a comment by CodeCov once
you opened a pull request and all the tests pass.

Continue to read this section if you want to calculate the test coverage by yourself.

First, [install `cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov?tab=readme-ov-file#installation)
if you haven't already installed.

Then run the normal tests with coverage instrumentation to generate a coverage report named `lcov.info`:

```bash
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

After that, run the root-only tests with coverage instrumentation.
We use [`bpfcov-rs`](https://github.com/kxxt/bpfcov-rs) to collect coverage of eBPF code that executes in kernel-space.

```bash
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER='sudo -E env TRACEXEC_BPFCOV_OUTDIR=/tmp/bpfcov'
cargo llvm-cov --all-features --workspace --lcov \
    --output-path root-lcov.info -- --ignored
```

After the tests finish,
- a user-space coverage report named `root-lcov.info` is produced,
- and kernel-space test coverage reports for each eBPF test is located
  in `/tmp/bpfcov`.

Then, combine all the kernel-space test coverage reports:

```bash
find /tmp/bpfcov -name '*.lcov' -print0 \
    | xargs -0 -I{} echo -a {} \
    | xargs lcov -o ebpf.lcov
```

And finally combine all three coverage reports into one:

```bash
lcov -a ebpf.lcov -a lcov.info -a root-lcov.info -o tracexec.info
```

Optionally you can generate an HTML report with:

```bash
genhtml tracexec.info --output-directory cov-out
```

## Add a Test

Feel free to add new tests to coverage new/modified code.

When adding a test that  requires root, please mark it with

```rust
#[ignore = "root"]
```

When the test loads eBPF program, please make sure that it runs sequentially
with respect to other eBPF tests by marking it with:

```rust
#[rstest]
#[file_serial(bpf)]
```

The outer `rstest` attribute is a workaround for getting the real test name.
