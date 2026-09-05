# Debugging a basic build problem

Sometimes a build fails even though you are sure you passed the right options.
In this tutorial, we will debug a small C project that cannot find its header file,
despite being given an include path.

We will use tracexec to find out what the build actually passed to the compiler,
then fix the problem and run the program.
You will need tracexec, GNU Make and a C compiler available as `cc`.

Here is a recording of the investigation and the fix:

<!-- From book/tutorials/basic-build-problem, run: asciinema record --window-size 100x26 --overwrite --command "bash --noprofile --norc" ../../casts/basic-build-problem.cast -->

{{ #asciinema ../casts/basic-build-problem.cast opts=casts/autoplay-loop.json }}

## The Example Project

Create a directory for the example:

```bash
mkdir -p basic-build-problem/include
cd basic-build-problem
```

It contains just three files:

```text
basic-build-problem/
├── Makefile
├── main.c
└── include/
    └── greeting.h
```

The complete source is below. You can also find these files in
`book/tutorials/basic-build-problem` in the tracexec repository.

`main.c`:

```c
{{#include basic-build-problem/main.c}}
```

`include/greeting.h`:

```c
{{#include basic-build-problem/include/greeting.h}}
```

`Makefile`:

```makefile
{{#include basic-build-problem/Makefile}}
```

The recipe lines must start with a tab. The `@` before the compiler command tells
make not to print that command, so we will only see the compiler's output when we build.
This Makefile has a small mistake that we will fix below.

## Reproducing the Failure

The header is in `include`, so let's pass `-Iinclude` through `CPPFLAGS`:

```bash
CPPFLAGS=-Iinclude make
```

With GCC, the output looks like this:

```text
main.c:2:10: fatal error: greeting.h: No such file or directory
    2 | #include "greeting.h"
      |          ^~~~~~~~~~~~
compilation terminated.
make: *** [Makefile:8: hello] Error 1
```

Clang reports the same missing header with slightly different wording.
The file exists and we supplied its directory. Did that option reach the compiler?

## Looking at the Compiler Invocation

Trace another build:

```bash
tracexec tui -- env CPPFLAGS=-Iinclude make
```

The `env` command sets `CPPFLAGS` for make and its children.
The compiler error will appear in the terminal pane, while the events pane shows the
programs involved in the build.

Switch to the `Events` pane with <kbd>Ctrl</kbd>+<kbd>S</kbd>.
Select the compiler invocation with <kbd>↑</kbd>/<kbd>↓</kbd> and press <kbd>V</kbd>
to open its details. In the recording, this is `/usr/bin/cc`, just after `make`.
If your compiler is GCC, you may also see a later `cc1` event; start with the `cc` invocation
that make launched.

In the `Info` tab, press <kbd>End</kbd> to scroll down to `Argv`.
The arguments look like this:

```text
["cc", "-Wall", "-Wextra", "main.c", "-o", "hello"]
```

The first argument may be a full path on your system.
The useful clue is that **`-Iinclude` is missing**.
The compiler was never told to search our header directory.

Now press <kbd>Tab</kbd> to switch to the `Environment` tab.
You should find:

```text
+"CPPFLAGS"="-Iinclude"
```

The `+` means that the variable was added relative to tracexec's starting environment.
If you already had `CPPFLAGS` set before starting tracexec, it may appear as modified
or unchanged instead.

So the environment variable reached the compiler, but its value did not appear in the arguments.
`CPPFLAGS` is a convention used by build tools: setting it does not add compiler options by itself.
The build recipe needs to pass those options on.

## Fixing the Makefile

Press <kbd>Q</kbd> to close the details, then <kbd>Q</kbd> again to leave tracexec.
Look at the compiler command in the Makefile:

```makefile
	@$(CC) $(CFLAGS) main.c -o $@
```

It uses `CFLAGS` but leaves out `CPPFLAGS`.
Change it to:

```makefile
	@$(CC) $(CPPFLAGS) $(CFLAGS) main.c -o $@
```

If you are following along from the terminal, this is the edit shown in the recording:

```bash
sed -i 's/$(CC) $(CFLAGS)/$(CC) $(CPPFLAGS) $(CFLAGS)/' Makefile
```

Now build and run it:

```bash
CPPFLAGS=-Iinclude make && ./hello
```

```text
Hello from the build tutorial!
```

The compiler now receives `-Iinclude` as an argument and finds `greeting.h`.
There is no need to clean before this retry because the failed build did not produce `hello`.
If you want to trace the successful compiler invocation too, repeat the tracing command
with `make -B` at the end to force a rebuild.

This example is small enough to spot the mistake by reading the Makefile.
In a larger build, tracexec lets you make the same check even when the compiler is launched
through several scripts or nested make invocations: find the exec event, inspect its arguments,
then check its environment and working directory in [Event Details](../features/tui/details.md).
