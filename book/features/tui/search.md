# Search

Search locates events by matching text in the Events pane. It supports literal
text and regular expressions, with optional case sensitivity. Matching events
are highlighted while the surrounding events remain visible, preserving the
context of each invocation.

## Search Procedure

With the Events pane focused, press <kbd>Ctrl</kbd>+<kbd>F</kbd> to open the
search prompt. Enter a query and press <kbd>Enter</kbd> to execute the search.

The first matching event is selected. Press <kbd>N</kbd> to move to the next
match or <kbd>P</kbd> to move to the previous match. The result counter reports
the position within the matching events and their total number; `2/5`, for
example, denotes the second of five matching events. Each event contributes
one result, even if the query occurs several times in its text. An unsuccessful
search displays `No match`.

Press <kbd>Ctrl</kbd>+<kbd>F</kbd> again to edit the query. The existing text
and matching options are retained. To close search and remove the highlights,
press <kbd>Esc</kbd> while editing. Thus, after submitting a query, use
<kbd>Ctrl</kbd>+<kbd>F</kbd> followed by <kbd>Esc</kbd> to close it. Submitting
an empty query has the same effect.

The following table lists the default bindings. They can be changed through
[Key Bindings](./keys.md).

| Context | Key | Action |
| --- | --- | --- |
| Main Events pane | <kbd>Ctrl</kbd>+<kbd>F</kbd> | Open or edit the search query. |
| Query editor | <kbd>Enter</kbd> | Submit the query. |
| Query editor | <kbd>Esc</kbd> | Close search and clear its results. |
| Query editor | <kbd>Ctrl</kbd>+<kbd>U</kbd> | Clear the query text. |
| Query editor | <kbd>Alt</kbd>+<kbd>I</kbd> | Toggle case sensitivity. |
| Query editor | <kbd>Alt</kbd>+<kbd>R</kbd> | Toggle literal text and regular-expression matching. |
| Events pane, after submission | <kbd>N</kbd> / <kbd>P</kbd> | Select the next or previous matching event. |

## Matching Rules

A new search uses case-insensitive literal matching by default. For example, `sample.c`
matches that text anywhere in an event line, including `SAMPLE.C`. The period
is treated as an ordinary character. With case sensitivity enabled, only the
specified letter case matches.

Regular-expression mode interprets the query as a pattern. For example,
`sample-[ab]\.c` matches `sample-a.c` or `sample-b.c`, while `gcc|clang`
matches either compiler name wherever it occurs in a line.
Case sensitivity can be changed independently of regular-expression mode.
The footer identifies the current modes while the query is being edited.

An invalid regex expression would cause a `Regex Error` popup to show.

The following recording compares literal and regular-expression matching for
`sample-[ab]\.c`. Enabling case sensitivity excludes `SAMPLE-A.C`; an invalid
pattern then illustrates error reporting and correction.

<!-- asciinema record --window-size 100x26 --command "tracexec --no-profile tui --layout vertical --filter exec -- bash --noprofile --norc" book/casts/tui-search-regex.cast -->

{{ #asciinema ../../casts/tui-search-regex.cast opts=casts/autoplay-loop.json }}

The displayed environment and working directory also form part of the search
text. After submission, <kbd>E</kbd> and <kbd>W</kbd> toggle these fields in
the Events pane and recompute the results. A hidden field does not contribute
matches.

## Example

Start a shell under tracexec.

```bash
tracexec tui -- bash --noprofile --norc
```

In the Terminal pane, execute the following commands:

```bash
/usr/bin/printf '%s\n' sample-a.c
/usr/bin/printf '%s\n' sample-b.c
/usr/bin/printf '%s\n' notes.txt
```

The explicit path invokes the external `printf` instead of shell builtin, so each command produces an
exec event. Switch to Events with <kbd>Ctrl</kbd>+<kbd>S</kbd>, open search
with <kbd>Ctrl</kbd>+<kbd>F</kbd>, and submit `sample-`. The first two `printf`
events match because their argument lists contain that text. The `notes.txt`
event remains visible but is not highlighted.

Use <kbd>N</kbd> and <kbd>P</kbd> to move between the results. To repeat the
search with a pattern, press <kbd>Ctrl</kbd>+<kbd>F</kbd>, clear the text with
<kbd>Ctrl</kbd>+<kbd>U</kbd>, enter `sample-[ab]\.c`, toggle regular-expression
mode with <kbd>Alt</kbd>+<kbd>R</kbd>, and press <kbd>Enter</kbd>.

<!-- asciinema record --window-size 100x26 --command "tracexec --no-profile tui --layout vertical --filter exec -- bash --noprofile --norc" book/casts/tui-search-text.cast -->

{{ #asciinema ../../casts/tui-search-text.cast opts=casts/autoplay-loop.json }}

## Real-time Search

An active search query is applied to newly arriving events. Collection continues
while the query is edited and while its results are inspected.

In this recording, the query is submitted before any matching events exist.
The result count increases as commands are executed in the Terminal pane.
An unrelated command leaves the count unchanged. A counter such as `0/2`
indicates that two matches have arrived but neither has been selected through
search navigation yet.

<!-- asciinema record --window-size 100x26 --command "tracexec --no-profile tui --layout vertical --filter exec --follow -- bash --noprofile --norc" book/casts/tui-search-live.cast -->

{{ #asciinema ../../casts/tui-search-live.cast opts=casts/autoplay-loop.json }}

