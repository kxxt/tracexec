{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    inputs@{
      flake-parts,
      crane,
      nixpkgs,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } (
      { withSystem, flake-parts-lib, ... }:
      let
        inherit (flake-parts-lib) importApply;
        tracexec.default = importApply ./nix/tracexec.nix { inherit crane; };
        ukci.default = importApply ./nix/ukci.nix { inherit nixpkgs; inherit crane; };
      in
      {
        imports = [
          tracexec.default
          ukci.default
        ];
        flake = { };
        systems = [
          "x86_64-linux"
          "aarch64-linux"
          "riscv64-linux"
        ];
        perSystem =
          {
            self',
            config,
            lib,
            pkgs,
            ...
          }:
          let
            defaultShell = pkgs.mkShell {
              name = "Development Shell";
              packages = with pkgs; [
                strace
                nixpkgs-fmt
                self'.packages.ukci
                self'.packages.run-qemu
                self'.packages.test-qemu
              ];
              shellHook = ''export TRACEXEC_LOGLEVEL=debug'';
            };
          in
          {
            packages.default = self'.packages.tracexec;
            devShells.default = defaultShell;
            devShells.extended = pkgs.mkShell {
              inputsFrom = [ defaultShell ];
              packages = [ 
                self'.packages.ukci-aarch64
                self'.packages.run-qemu-aarch64
                self'.packages.test-qemu-aarch64
              ];
            };
          };
      }
    );
}
