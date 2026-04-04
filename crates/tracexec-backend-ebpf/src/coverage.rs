//! eBPF program coverage collection (gated behind the `bpfcov` feature).
//!
//! When the crate is built with `--features bpfcov`, BPF programs are compiled
//! with profiling instrumentation.  After running the programs, call
//! [`write_coverage`] to dump a profraw file that can be post-processed with
//! `llvm-profdata` and `llvm-cov`.

use std::{
  io,
  path::Path,
};

use bpfcov::collect::collect_coverage_data;
use libbpf_rs::Object;

/// Path to the coverage-only `.bpf.obj` file (set by `build.rs` via `cargo:rustc-env`).
///
/// This object is needed by `llvm-cov show` to correlate profraw data with
/// source locations.
pub const COVERAGE_OBJ: &str = env!("BPFCOV_COV_OBJ");

/// Collect coverage data from the loaded BPF object and write it as a profraw
/// v10 file.
pub fn write_coverage(obj: &Object, output: &Path) -> io::Result<()> {
  let data = collect_coverage_data(obj).map_err(|e| io::Error::other(e.to_string()))?;
  let mut file = std::fs::File::create(output)?;
  data.write_profraw(&mut file)
}

/// Generate an HTML coverage report by merging the profraw into profdata and
/// running `llvm-cov show`.
///
/// Returns the path to the generated `index.html` inside `output_dir`.
pub fn generate_report(profraw: &Path, output_dir: &Path) -> io::Result<std::path::PathBuf> {
  let profdata = output_dir.join("coverage.profdata");
  let cov_obj = Path::new(COVERAGE_OBJ);

  bpfcov::report::merge_profdata(&[profraw], &profdata, None)?;
  bpfcov::report::generate_html_report(&profdata, &[cov_obj], output_dir, None)?;

  Ok(output_dir.join("index.html"))
}

/// Export coverage data in LCOV tracefile format.
///
/// Merges the profraw into profdata, then runs `llvm-cov export --format=lcov`.
/// Returns the path to the generated `.lcov` file.
pub fn export_lcov(profraw: &Path, output_dir: &Path) -> io::Result<std::path::PathBuf> {
  let profdata = output_dir.join("coverage.profdata");
  let lcov_path = output_dir.join("coverage.lcov");
  let cov_obj = Path::new(COVERAGE_OBJ);

  bpfcov::report::merge_profdata(&[profraw], &profdata, None)?;
  bpfcov::report::export_lcov(&profdata, &[cov_obj], &lcov_path, None)?;

  Ok(lcov_path)
}
