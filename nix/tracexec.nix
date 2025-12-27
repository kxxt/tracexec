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
          sourceFilter =
            path: type:
            (cFilter path type)
            || (protoFilter path type)
            || (symlinkFilter path type)
            || (craneLib.filterCargoSources path type);
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
          tracexec_no_rcu_kfuncs = builder { cargoExtraArgs = "--locked -F ebpf-no-rcu-kfuncs"; };
        };
    };
}
