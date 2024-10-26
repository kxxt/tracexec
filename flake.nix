{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs@{ flake-parts, crane, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } (
      { withSystem, flake-parts-lib, ... }:
      let inherit (flake-parts-lib) importApply;
        tracexec.default = importApply ./nix/tracexec.nix { inherit crane; };
      in
      {
        imports = [
          tracexec.default
        ];
        flake = {
        };
        systems = [
          "x86_64-linux"
          "aarch64-linux"
          "riscv64-linux"
        ];
        perSystem = { self', config, lib, pkgs, ... }: {
          packages.default = self'.packages.tracexec;
          devShells.default = pkgs.mkShell {
            name = "Development Shell";
            packages = with pkgs; [
              strace
            ];
            shellHook = ''export TRACEXEC_LOGLEVEL=debug'';
          };
        };
      }
    );
}
