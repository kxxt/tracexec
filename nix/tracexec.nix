localFlake:

{ lib, config, self, inputs, ... }:
{
  perSystem = { system, pkgs, ... }: {
    packages.tracexec =
      let
        craneLib = localFlake.crane.mkLib pkgs;
        cFilter = path: _type: builtins.match ".*\.[ch]$" path != null;
        symlinkFilter = _path: type: type == "symlink";
        sourceFilter = path: type:
          (cFilter path type) || (symlinkFilter path type) || (craneLib.filterCargoSources path type);
      in
      craneLib.buildPackage {
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
        ];
        hardeningDisable = [
          "zerocallusedregs"
        ];
        # Don't store logs
        TRACEXEC_DATA = "/tmp";
      };
  };
}
