# Key Bindings

You can customize the key bindings in the config file.

The key bindings of the TUI are defined in the `tui.keys` section, as shown in the following example:

```toml
[tui.keys]
quit = "q"
switch_pane = "Ctrl+s"
# switch_layout = "Alt+l"
# close_popup = "q"
# help = "F1"
# page_down = ["Ctrl+Down", "Ctrl+j", "PgDn"]
# page_up = ["Ctrl+Up", "Ctrl+k", "PgUp"]
```

## Configuration Format

For each action, you can bind a single or multiple key bindings to it.

For example, `switch_pane = "Ctrl+s"` binds <kbd>Ctrl</kbd>+<kbd>S</kbd>
to `switch_pane` action.
`page_up = ["Ctrl+Up", "Ctrl+k", "PgUp"]` binds multiple shortcuts to the `page_up` action.

## Supported Key Bindings

The supported key bindings are listed in <https://github.com/kxxt/tracexec/blob/main/crates/tracexec-core/src/cli/keys.rs>.
