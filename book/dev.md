# Developer Guide

This part of the book is for contributors changing tracexec itself.

Start with [Internal Architecture](./dev/architecture.md) for the crate layout,
then read [Backend Differences](./dev/backend-differences.md) before changing
code shared by ptrace and eBPF. [Event System](./dev/event-system.md) documents
the messages passed from a backend to a frontend and the parent links used by
the TUI.

The remaining chapters cover the practical work:

- [Tests](./dev/tests.md) explains the normal, privileged, eBPF, and verifier
  test suites.
- [Checklist for Cutting a Release](./dev/release.md) lists the release steps.
- [Maintaining this Book](./dev/book.md) describes the local book workflow and
  media policy.

All crates in this workspace are implementation details. If code looks useful
outside tracexec, discuss extracting a supported library before depending on an
internal crate.
