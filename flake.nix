{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs@{ flake-parts, crane, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      flake = {

      };
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "riscv64-linux"
      ];
      perSystem = { config, lib, pkgs, ... }:
        let
          cargoToml = lib.importTOML ./Cargo.toml;
          craneLib = crane.mkLib pkgs;
        in
        {
          packages.default =
            let
              cFilter = path: _type: builtins.match ".*\.[ch]$" path != null;
              symlinkFilter = _path: type: type == "symlink";
              sourceFilter = path: type:
                (cFilter path type) || (symlinkFilter path type) || (craneLib.filterCargoSources path type);
            in
            craneLib.buildPackage {
              src = lib.cleanSourceWith {
                src =  ./.;
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

          devShells.default = pkgs.mkShell {
            name = "Development Shell";
            packages = with pkgs; [
              strace
            ];
            shellHook = ''export TRACEXEC_LOGLEVEL=debug'';
          };
        };
  };
}
