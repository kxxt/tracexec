bump level:
  cargo release version --no-confirm -x {{level}}

update-readme:
  #!/usr/bin/env ruby

  readme = File.read "README.template.md"

  tracexec = "cargo run -- "

  helps = {
    :general => `#{tracexec} --help`,
    :tui => `#{tracexec} tui --help`,
    :log => `#{tracexec} log --help`,
    :collect => `#{tracexec} collect --help`,
    :ebpf => `#{tracexec} ebpf --help`
  }

  File.write("README.md", readme % helps)

arch-family := if arch() == "x86_64" {
  "x86"
} else if arch() == "aarch64" {
  "arm64"
} else if arch() == "riscv64" {
  "riscv"
} else {
  error("Unsupported architecture")
}

bpf-disasm debug:
  clang -I crates/tracexec-backend-ebpf/include \
   -D TRACEXEC_TARGET_{{uppercase(arch())}} -DMAX_CPUS=64 {{ if debug == "debug" { "-DEBPF_DEBUG" } else { "" } }} \
   -I crates/tracexec-backend-ebpf/src/bpf/src -fno-stack-protector \
   -D__TARGET_ARCH_{{arch-family}} -g -O2 -target bpf \
   crates/tracexec-backend-ebpf/src/bpf/tracexec_system.bpf.c \
   -S -o /dev/stdout
