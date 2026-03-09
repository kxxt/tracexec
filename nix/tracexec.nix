localFlake:

{
  lib,
  config,
  self,
  inputs,
  ...
}:
{
  perSystem =
    { system, pkgs, ... }:
    {
      packages =
        let
          craneLib = localFlake.crane.mkLib pkgs;
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
              && (base != "config.toml" || parentDir != ".cargo") # Filter out .cargo/config.toml
            )
            || base == "Cargo.lock";
          sourceFilter =
            path: type:
            (cFilter path type)
            || (protoFilter path type)
            || (symlinkFilter path type)
            || (rustFilter path type);
          builder =
            { cargoExtraArgs }:
            craneLib.buildPackage {
              inherit cargoExtraArgs;
              src = lib.cleanSourceWith {
                src = ./..;
                filter = sourceFilter;
                name = "source";
              };
              buildInputs = with pkgs; [
                elfutils
                zlib
                libseccomp
              ];
              nativeBuildInputs = with pkgs; [
                pkg-config
                clang # For building eBPF
                protobuf # For generating binding to perfetto protos
              ];
              hardeningDisable = [
                "zerocallusedregs"
              ];
              # Don't store logs
              TRACEXEC_DATA = "/tmp";
            };
        in
        {
          tracexec = builder { cargoExtraArgs = "--locked"; };
        };
    };
}
