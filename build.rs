fn main() {
  #[cfg(feature = "ebpf")]
  {
    use std::{
      env,
      ffi::{OsStr, OsString},
      path::PathBuf,
    };

    use libbpf_cargo::SkeletonBuilder;

    const BPF_SRC: &str = "src/bpf/tracexec_system.bpf.c";
    let manifest_dir =
      PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let skel_out = manifest_dir
      .clone()
      .join("src")
      .join("bpf")
      .join("tracexec_system.skel.rs");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH not set");
    let arch_define = OsStr::new(match arch.as_str() {
      "x86_64" => "__x86_64__",
      "riscv64" => "__riscv64__",
      "aarch64" => "__aarch64__",
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
    SkeletonBuilder::new()
      .source(BPF_SRC)
      .clang_args(clang_args)
      .build_and_generate(&skel_out)
      .unwrap();
    println!("cargo:rerun-if-changed={BPF_SRC}");
    println!("cargo:rerun-if-changed=src/bpf/common.h");
    println!("cargo:rerun-if-changed=src/bpf/interface.h");
  }
}
