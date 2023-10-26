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
      --successful-only   Only show successful calls
      --show-cmdline      Print commandline that reproduces what was executed. Note that when filename and argv[0] differs, it probably won't give you the correct commandline for now. Implies --successful-only
      --show-interpreter  Try to show script interpreter indicated by shebang
      --more-colors       More colors
      --less-colors       Less colors
      --show-children     Print a message when a child is created
      --diff-env          Diff environment variables with the original environment
      --no-diff-env       Do not diff environment variables
      --show-env          Trace environment variables
      --no-show-env       Do not trace environment variables
      --show-comm         Show comm
      --no-show-comm      Do not show comm
      --show-argv         Show argv
      --no-show-argv      Do not show argv
      --show-filename     Show filename
      --no-show-filename  Do not show filename
      --show-cwd          Show cwd
      --no-show-cwd       Do not show cwd
      --decode-errno      Decode errno values
      --no-decode-errno   
  -h, --help              Print help
```

## Origin

This project was born out of the need to trace the execution of programs.

Initially I simply use `strace -Y -f -qqq -s99999 -e trace=execve,execveat <command>`.

But the output is still too verbose so that's why I created this project.

## Credits

This project takes inspiration from [strace](https://strace.io/) and [lurk](https://github.com/JakWai01/lurk).
