# Installation

Different people have different opinions when it comes to how to install a program.
Some may prefer using system package manager while others might like downloading a prebuilt binary.
But don't worry, tracexec supports a wide variety of installation methods.

Before installation, you can check [the platform support status](./support.md).

## Install via Package Managers

tracexec is packaged in the following distributions.

[![Packaging status](https://repology.org/badge/vertical-allrepos/tracexec.svg)](https://repology.org/project/tracexec/versions)

### Arch Linux (And Arch-based distributions)

tracexec is available in `extra` repository for Arch Linux. You can install it via

```bash
sudo pacman -S tracexec
```

### Nix

To try tracexec without system-wide installation, running

```bash
nix-shell -p tracexec
```

will drop you into a shell where tracexec is available.


### NixOS

If you are using NixOS, you should already have your preferred way to install packages.

e.g. by adding `pkgs.tracexec` to `environment.systemPackages`

## Prebuilt Binaries

For stable versions, we release binaries in [GitHub Releases](https://github.com/kxxt/tracexec/releases).

Currently we offer two flavors of binaries

- Normal builds that dynamically links most dependencies except `libbpf`.
- Fully statically-linked builds which statically links all libraries including `glibc`.

## Install from Source

Please refer to [Building from Source](./build-from-src.md) for dependencies and feature flags.

To install the current stable version of tracexec. Run 

```bash
cargo install tracexec --bin tracexec
```

To install the bleeding-edge main branch git version of tracexec. Run

```bash
cargo install --git https://github.com/kxxt/tracexec --bin tracexec
```
