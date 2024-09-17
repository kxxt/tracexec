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
