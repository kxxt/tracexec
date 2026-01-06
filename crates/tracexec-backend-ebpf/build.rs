fn main() {
  use std::{
    env,
    ffi::{OsStr, OsString},
    path::PathBuf,
  };

  use libbpf_cargo::SkeletonBuilder;

  const BPF_SRC: &str = "src/bpf/tracexec_system.bpf.c";
  let manifest_dir =
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
  let bpf_src = manifest_dir.join(BPF_SRC);
  let skel_out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"))
    .join("tracexec_system.skel.rs");
  let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
  let arch_define = OsStr::new(match arch.as_str() {
    "x86_64" => "TRACEXEC_TARGET_X86_64",
    "riscv64" => "TRACEXEC_TARGET_RISCV64",
    "aarch64" => "TRACEXEC_TARGET_AARCH64",
    _ => panic!("Arch {arch} is not supported for now"),
  });
  let max_cpus = 64;
  let max_cpus_define = OsString::from(format!("MAX_CPUS={max_cpus}"));
  let include_dir = manifest_dir.join("include");
  let mut clang_args = vec![
    // vmlinux.h
    OsStr::new("-I"),
    include_dir.as_os_str(),
    OsStr::new("-D"),
    arch_define,
    OsStr::new("-D"),
    &max_cpus_define,
  ];
  if cfg!(any(feature = "ebpf-debug", debug_assertions)) {
    clang_args.push(OsStr::new("-DEBPF_DEBUG"));
  }
  if cfg!(feature = "ebpf-no-rcu-kfuncs") {
    clang_args.push(OsStr::new("-DNO_RCU_KFUNCS"));
  }
  let mut builder = SkeletonBuilder::new();
  builder.source(bpf_src).clang_args(clang_args);
  if let Some(path) = std::env::var_os("CLANG") {
    builder.clang(path);
  }
  builder.build_and_generate(&skel_out).unwrap();
  println!("cargo:rerun-if-env-changed=CLANG");
  println!("cargo:rerun-if-changed={BPF_SRC}");
  println!("cargo:rerun-if-changed=src/bpf/common.h");
  println!("cargo:rerun-if-changed=src/bpf/interface.h");
}
