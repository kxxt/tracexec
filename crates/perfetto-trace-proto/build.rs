fn main() {
  // Generate protobuf bindings
  prost_build::compile_protos(
    &["perfetto/protos/perfetto/trace/trace.proto"],
    &["perfetto/"],
  )
  .expect("Failed to build protobuf bindings");
  println!("cargo:rerun-if-changed=perfetto/protos");
}
