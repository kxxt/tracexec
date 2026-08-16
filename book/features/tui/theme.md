# Theme

The TUI supports custom themes.
A theme is defined in a theme file
and specified in the config file as follows.

```toml
[tui]
theme-file = "nord.toml"
```

## Theme File Resolution

Absolute paths are used as-is.

Relative paths are resolved relative to the theme directories, in the following order:
  1. `$XDG_CONFIG_HOME/tracexec/themes/` (or `$HOME/.config/tracexec/themes/`)
  2. `$XDG_DATA_HOME/tracexec/themes/` (or `$HOME/.local/share/tracexec/themes/`)
  3. `/etc/tracexec/themes/`
  4. `<path_to_tracexec_binary>/../share/tracexec/themes/` (usually `/usr/share/tracexec/themes/`)

## Theme File Format

The theme file is written in TOML and the TUI theme is put under a section named `tui`, as demonstrated by the following example:


```toml
# Cool blue theme inspired by the Nord palette.

[tui]
inactive-border = { fg = "#4c566a" }
active-border = { fg = "#88c0d0", modifiers = ["bold"] }
app-title = { fg = "#eceff4" }
help-popup = { fg = "#2e3440", bg = "#88c0d0" }
cli-flag = { fg = "#2e3440", bg = "#81a1c1" }
help-key = { fg = "#2e3440", bg = "#88c0d0" }
help-desc = { fg = "#d8dee9", bg = "#3b4252", remove-modifiers = ["italic"] }
pid-success = { fg = "#a3be8c" }
pid-failure = { fg = "#bf616a" }
pid-enoent = { fg = "#ebcb8b" }
comm = { fg = "#88c0d0" }
tracer-info = { fg = "#81a1c1" }
tracer-warning = { fg = "#ebcb8b" }
tracer-error = { fg = "#bf616a" }
new-child-pid = { fg = "#8fbcbb" }
tracer-event = { fg = "#b48ead" }
partial-ok = { fg = "#ebcb8b", modifiers = ["italic"] }
filename = { fg = "#88c0d0" }
cwd = { fg = "#8fbcbb" }
modified-env-var = { fg = "#ebcb8b" }
added-env-var = { fg = "#a3be8c" }
query-match-current-no = { fg = "#88c0d0", modifiers = ["bold"] }
query-match-total-cnt = { fg = "#d8dee9" }
breakpoint-title-selected = { fg = "#2e3440", bg = "#81a1c1" }
breakpoint-pattern = { fg = "#88c0d0" }
breakpoint-info-value = { fg = "#2e3440", bg = "#8fbcbb" }
hit-entry-breakpoint-pattern = { fg = "#88c0d0" }
hit-manager-default-command = { fg = "#8fbcbb" }
active-tab = { fg = "#2e3440", bg = "#81a1c1" }
backtrace-parent-spawns = { content = " S ", fg = "#2e3440", bg = "#88c0d0", modifiers = ["bold"] }
backtrace-parent-becomes = { content = " B ", fg = "#eceff4", bg = "#5e81ac", modifiers = ["bold"] }
```

The theme file is applied as an override to the built-in theme. That is,
the styles are merged with the built-in theme and unspecified entries will use the built-in theme.

A theme entry specifies the style of a UI element. It supports the following attributes.

- `fg`: foreground color
- `bg`: background color
- `underline-color`: color of underline decoration
- `modifiers`: a list of modifiers to apply.
- `remove-modifiers`: a list of modifiers to remove from the built-in theme.

For colors, the following formats are supported.

- An unsigned 8-bit integer representing an [8-bit color](https://en.wikipedia.org/wiki/ANSI_escape_code#8-bit).
- A named color listed in <https://docs.rs/ratatui-core/0.1.2/ratatui_core/style/enum.Color.html#variants>. (Kebab-case should be used here)
- A hex color string in `#RRGGBB` format.
- A dict specifying the rgb color separately, like `{ r = 255, g = 0, b = 0 }`

The following modifiers are supported:

- `bold`
- `dim`
- `italic`
- `underlined`
- `slow-blink`
- `rapid-blink`
- `reversed`
- `hidden`
- `crossed-out`

Some theme entries support specifying the content of the UI element.

```toml
backtrace-parent-spawns = { content = " S ", bg = "red" }
```

### Supported Theme Entries

The supported theme entries are listed in <https://github.com/kxxt/tracexec/blob/main/crates/tracexec-core/src/cli/tui_theme.rs>.

## Theme in Config File

The theme could also be specified directly in the config file,
as shown in the following example.


```toml
[tui]
theme = { app-title = { fg = "cyan" }, active-border = { fg = "light-cyan" } }
```
