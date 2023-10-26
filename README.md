# tracexec

A small utility to trace program execution.

**Status**:

- Proof of concept.
- Experimental quality.
- Not ready for production use.
- Performance is not a focus right now.

## Showcases

### Default mode

By default, `tracexec` will print filename, argv and the diff of the environment variables.

[![asciicast](https://asciinema.org/a/5ZH5pAPTdTeSXIWIZmm015UNr.svg)](https://asciinema.org/a/5ZH5pAPTdTeSXIWIZmm015UNr)

### Reconstruct the command line with `--print-cmdline`

```bash
$ tracexec log --print-cmdline -- <command>
```

[![asciicast](https://asciinema.org/a/k8lXyeF19Es7cLO4RUw0Cu4OU.svg)](https://asciinema.org/a/k8lXyeF19Es7cLO4RUw0Cu4OU)

### Show the interpreter indicated by shebang with `--trace-interpreter`

```bash
$ tracexec log --trace-interpreter -- <command>
```

[![asciicast](https://asciinema.org/a/nkvDleC3nyVOT2Cif8nOXBuVV.svg)](https://asciinema.org/a/nkvDleC3nyVOT2Cif8nOXBuVV)

## Installation

Via cargo:

```bash
cargo install 'tracexec@0.0.1'
```

## Usage

```bash
Run tracexec in logging mode

Usage: tracexec log [OPTIONS] -- <CMD>...

Arguments:
  <CMD>...  command to be executed

Options:
      --successful-only    Only show successful calls
      --print-cmdline      Print commandline that reproduces what was executed. Note that when filename and argv[0] differs, it won't give you the correct commandline for now. Implies --successful-only
      --trace-interpreter  Try to trace script interpreter
      --more-colors        More colors
      --less-colors        Less colors
      --print-children     Print a message when a child is created
      --diff-env           Diff environment variables with the original environment
      --no-diff-env        Do not diff environment variables
      --trace-env          Trace environment variables
      --no-trace-env       Do not trace environment variables
      --trace-comm         Trace comm
      --no-trace-comm      Do not trace comm
      --trace-argv         Trace argv
      --no-trace-argv      Do not trace argv
      --trace-filename     Trace filename
      --no-trace-filename  Do not trace filename
      --trace-cwd          Trace cwd
      --no-trace-cwd       Do not trace cwd
      --decode-errno       Decode errno values
      --no-decode-errno    
  -h, --help               Print help
```

## Origin

This project was born out of the need to trace the execution of programs.

Initially I simply use `strace -Y -f -qqq -s99999 -e trace=execve,execveat <command>`.

But the output is still too verbose so that's why I created this project.

## Credits

This project takes inspiration from [strace](https://strace.io/) and [lurk](https://github.com/JakWai01/lurk).
