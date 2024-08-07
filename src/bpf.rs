pub mod skel {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bpf/tracexec_system.skel.rs"
    ));
}
