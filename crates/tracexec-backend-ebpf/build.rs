fn main() {
  use std::{
    env,
    ffi::{
      OsStr,
      OsString,
    },
    path::PathBuf,
  };

  const BPF_SRC: &str = "src/tracexec_system.bpf.c";
  let manifest_dir =
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
  let bpf_src = manifest_dir.join(BPF_SRC);
  let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
  let skel_out = out_dir.join("tracexec_system.skel.rs");
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
  let bpf_cflags = env::var("BPF_CFLAGS").ok();
  if let Some(bpf_cflags) = bpf_cflags.as_deref() {
    clang_args.extend(bpf_cflags.split_ascii_whitespace().map(OsStr::new));
  }
  if cfg!(any(feature = "ebpf-debug", debug_assertions)) {
    clang_args.push(OsStr::new("-DEBPF_DEBUG"));
  }

  if cfg!(feature = "bpfcov") {
    build_with_bpfcov(&bpf_src, &out_dir, &skel_out, &clang_args);
  } else {
    build_normal(&bpf_src, &out_dir, &skel_out, clang_args);
  }

  println!("cargo:rerun-if-env-changed=CLANG");
  println!("cargo:rerun-if-env-changed=BPF_CFLAGS");
  println!("cargo:rerun-if-changed={BPF_SRC}");
  println!("cargo:rerun-if-changed=src/bpf/common.h");
  println!("cargo:rerun-if-changed=src/bpf/interface.h");
}

fn build_normal(
  bpf_src: &std::path::Path,
  out_dir: &std::path::Path,
  skel_out: &std::path::Path,
  clang_args: Vec<&std::ffi::OsStr>,
) {
  use libbpf_cargo::SkeletonBuilder;

  let mut builder = SkeletonBuilder::new();
  builder.reference_obj(true);
  // TODO: drop the following line when https://github.com/libbpf/libbpf-rs/pull/1354 lands
  builder.obj(out_dir.join("tracexec_system.o"));
  builder.source(bpf_src).clang_args(clang_args);
  if let Some(path) = std::env::var_os("CLANG") {
    builder.clang(path);
  }
  builder.build_and_generate(skel_out).unwrap();
}

#[cfg(feature = "bpfcov")]
fn build_with_bpfcov(
  bpf_src: &std::path::Path,
  out_dir: &std::path::Path,
  skel_out: &std::path::Path,
  clang_args: &[&std::ffi::OsStr],
) {
  use std::path::PathBuf;

  use bpfcov::instrument::Pipeline;
  use libbpf_cargo::SkeletonBuilder;

  let lib_bpfcov = std::env::var_os("BPFCOV_LIB").map(PathBuf::from);

  // Step 1: Normal build + skeleton generation (produces valid Rust types)
  let obj_path = out_dir.join("tracexec_system.o");
  let mut builder = SkeletonBuilder::new();
  builder.reference_obj(true);
  builder.obj(&obj_path);
  builder.source(bpf_src).clang_args(clang_args.to_vec());
  if let Some(path) = std::env::var_os("CLANG") {
    builder.clang(path);
  }
  builder.build().unwrap();
  builder.generate(skel_out).unwrap();

  // Step 2: Build instrumented object via the bpfcov pipeline
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
  let bpf_arch = match arch.as_str() {
    "x86_64" => "x86",
    "aarch64" => "arm64",
    "powerpc64" => "powerpc",
    "s390x" => "s390",
    "riscv64" => "riscv",
    "loongarch64" => "loongarch",
    x => x,
  };
  let bpf_arch_define = format!("-D__TARGET_ARCH_{bpf_arch}");

  let cov_dir = out_dir.join("cov");
  let mut pipeline = Pipeline::new();
  if let Some(lib_bpfcov) = &lib_bpfcov {
    pipeline = pipeline.lib_bpfcov(lib_bpfcov);
  }
  let result = pipeline
    .source(bpf_src)
    .output_dir(&cov_dir)
    .clang_args(clang_args.iter().map(|s| s.to_os_string()))
    .clang_arg(&bpf_arch_define)
    .clang_arg("-fno-stack-protector")
    .run()
    .expect("bpfcov instrumentation pipeline failed");

  // Step 3: Replace the .o with the instrumented version.
  // The skeleton's include_bytes!() references this path at compile time,
  // so the final binary embeds the instrumented object.
  std::fs::copy(&result.instrumented_obj, &obj_path)
    .expect("failed to overwrite .o with instrumented object");

  // Step 4: Patch the skeleton to tolerate extra bpfcov maps and the
  // different object size.
  let skel = std::fs::read_to_string(skel_out).expect("failed to read skeleton");
  // Allow unknown maps (bpfcov adds .data.profc, .rodata.profd, etc.)
  let panic_pattern = r#"_ => panic!("encountered unexpected map: `{name}`"),"#;
  assert!(
    skel.contains(panic_pattern),
    "skeleton panic pattern not found — libbpf-cargo may have changed its codegen"
  );
  let skel = skel.replace(panic_pattern, "_ => {},");
  // Fix the DATA array size to match the instrumented object
  let instrumented_size = std::fs::metadata(&obj_path).unwrap().len() as usize;
  let size_pattern = regex_lite::Regex::new(r"static DATA: \[u8; \d+\]").unwrap();
  assert!(
    size_pattern.is_match(&skel),
    "skeleton DATA array pattern not found — libbpf-cargo may have changed its codegen"
  );
  let skel = size_pattern.replace(&skel, format!("static DATA: [u8; {instrumented_size}]"));
  std::fs::write(skel_out, skel.as_bytes()).expect("failed to write patched skeleton");

  // Expose the coverage object path so runtime code can find it
  println!(
    "cargo:rustc-env=BPFCOV_COV_OBJ={}",
    result.coverage_obj.display()
  );
  println!("cargo:rerun-if-env-changed=BPFCOV_LIB");
}

#[cfg(not(feature = "bpfcov"))]
fn build_with_bpfcov(
  _bpf_src: &std::path::Path,
  _out_dir: &std::path::Path,
  _skel_out: &std::path::Path,
  _clang_args: &[&std::ffi::OsStr],
) {
  unreachable!()
}
