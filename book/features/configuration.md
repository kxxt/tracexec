# Configuration Profile

If you find yourself passing the same options every time you run tracexec, you
can put them in a configuration file. For example, you might want the TUI to
start with the Events pane focused, or always show timestamps in the log.

## Configuring tracexec

tracexec looks for `config.toml` in `$XDG_CONFIG_HOME/tracexec/`, or
`$HOME/.config/tracexec/` if `XDG_CONFIG_HOME` is not set.

Create the directory if needed:

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/tracexec"
```

[A configuration template](https://github.com/kxxt/tracexec/blob/main/config.toml)
is provided in the repository, which documents all available options with their
default values and descriptions.

The file uses TOML. Setting names usually use underscores, such as
`active_pane`, while CLI flags use hyphens, such as `--active-pane`.
Enum values are case-sensitive: write `"Events"` in TOML, even though the CLI
spelling is `--active-pane events`. The tables below use the TOML spellings.

You can start with the configuration template and adjust settings according to your preference.

## Using a Different Profile

Use `--profile` (or `-P`) to load a different profile than the default profile; for example:

```bash
tracexec --profile ./build.toml tui -- bash
```

This loads `build.toml` instead of the default `config.toml`. The two files are
not merged, so a setting omitted from the profile uses its built-in default.

Relative profile paths are resolved from tracexec's working directory.
`--cwd` changes that directory before loading the profile. For example,
`tracexec --cwd /path/to/project --profile build.toml tui -- bash` reads
`/path/to/project/build.toml` and starts Bash in that directory.

To run with the built-in defaults, use `--no-profile`:

```bash
tracexec --no-profile log -- ls
```

When using [privilege elevation](./elevation.md) through `--elevate`, tracexec
preserves the original user's configuration directory.
