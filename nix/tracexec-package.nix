{ pkgs, lib, crane }:
{ cargoExtraArgs ? "--locked --no-default-features -F recommended" }:
let
  craneLib = crane.mkLib pkgs;
  isLibbpfSys =
    p:
    p.name == "libbpf-sys"
    && p.version == "1.6.3+v1.6.3";
  cFilter = path: _type: builtins.match ".*\.[ch]$" path != null;
  protoFilter = path: _type: builtins.match ".*\.proto$" path != null;
  symlinkFilter = _path: type: type == "symlink";
  rustFilter =
    path: type:
    let
      base = baseNameOf path;
      parentDir = baseNameOf (dirOf path);
    in
    type == "directory"
    || (
      lib.any (suffix: lib.hasSuffix suffix base) [
        ".rs"
        ".toml"
      ]
      # Filter out .cargo/config.toml which contains config for cross-compilers.
      # Nix will set them up.
      && (base != "config.toml" || parentDir != ".cargo") 
    )
    || base == "Cargo.lock";
  sourceFilter =
    path: type:
    (cFilter path type)
    || (protoFilter path type)
    || (symlinkFilter path type)
    || (rustFilter path type);
  baseArgs = {
    src = lib.cleanSourceWith {
      src = ./..;
      filter = sourceFilter;
      name = "source";
    };
  };
  cargoVendorDir = craneLib.vendorCargoDeps (baseArgs // {
    overrideVendorCargoPackage =
      p: drv:
      if isLibbpfSys p then
        drv.overrideAttrs (_old: {
          patches = [
            ./patches/libbpf-sys-pkg-config.patch
          ];
        })
      else
        drv;
  });
in
craneLib.buildPackage {
  inherit cargoExtraArgs cargoVendorDir;
  inherit (baseArgs) src;
  buildInputs = with pkgs; [
    elfutils
    zlib
    libseccomp
    libbpf
    linuxHeaders
    # For building libbpf-cargo (it runs on build)
    pkgs.buildPackages.libbpf
    pkgs.buildPackages.zlib
    pkgs.buildPackages.elfutils
  ];
  nativeBuildInputs = [
    pkgs.buildPackages.pkg-config
    # For building eBPF, use unwrapped binary since bpf != target arch
    pkgs.buildPackages.clang.cc
    # For generating binding to perfetto protos
    # pkgs.buildPackages.protobuf
  ];
  hardeningDisable = [
    "zerocallusedregs"
  ];
  BPF_CFLAGS = "-isystem ${pkgs.linuxHeaders}/include -I ${pkgs.libbpf}/include";
  # Don't store logs
  TRACEXEC_DATA = "/tmp";
}
