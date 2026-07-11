{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    flake-root.url = "github:srid/flake-root";
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
        ukci.default = importApply ./nix/ukci.nix {
          inherit nixpkgs;
          inherit crane;
        };
      in
      {
        imports = [
          tracexec.default
          ukci.default
          inputs.treefmt-nix.flakeModule
          inputs.flake-root.flakeModule
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
            system,
            ...
          }:
          let
            plotVerifierComplexity = pkgs.writeShellApplication {
              name = "plot-verifier-complexity";
              runtimeInputs = [
                (pkgs.python3.withPackages (pythonPackages: [
                  pythonPackages.matplotlib
                ]))
              ];
              text = ''
                script="''${TRACEXEC_PLOT_VERIFIER_COMPLEXITY_SCRIPT:-scripts/plot-verifier-complexity.py}"
                if [ ! -f "$script" ]; then
                  echo "plot-verifier-complexity: could not find $script" >&2
                  echo "run this from the tracexec repository root, or set TRACEXEC_PLOT_VERIFIER_COMPLEXITY_SCRIPT" >&2
                  exit 1
                fi
                exec python3 "$script" "$@"
              '';
            };
            defaultShell = pkgs.mkShell {
              name = "Development Shell";
              packages =
                with pkgs;
                [
                  strace
                  nixfmt
                  statix
                  config.treefmt.build.wrapper
                  self'.packages.ukci
                  self'.packages.run-qemu
                  self'.packages.test-qemu
                ]
                ++ builtins.attrValues config.treefmt.build.programs;
              shellHook = "export TRACEXEC_LOGLEVEL=debug";
            };
          in
          {
            packages.default = self'.packages.tracexec;
            packages.plot-verifier-complexity = plotVerifierComplexity;
            devShells.default = defaultShell;
            devShells.extended = pkgs.mkShell {
              inputsFrom = [ defaultShell ];
              packages =
                lib.optionals (system != "aarch64-linux") [
                  self'.packages.ukci-aarch64
                  self'.packages.run-qemu-aarch64
                  self'.packages.test-qemu-aarch64
                ]
                ++ lib.optionals (system != "x86_64-linux") [
                  self'.packages.ukci-x86_64
                  self'.packages.run-qemu-x86_64
                  self'.packages.test-qemu-x86_64
                ]
                ++ lib.optionals (system != "riscv64-linux") [
                  self'.packages.ukci-riscv64
                  self'.packages.run-qemu-riscv64
                  self'.packages.test-qemu-riscv64
                ];
            };
            devShells.cross = pkgs.mkShell {
              inputsFrom = [ defaultShell ];
              packages =  [
                self'.packages.ukci-latest-llvm
              ]
              ++ lib.optionals (system != "aarch64-linux") [
                self'.packages.ukci-aarch64-latest-llvm
                self'.packages.run-qemu-aarch64-latest-llvm
                self'.packages.test-qemu-aarch64-latest-llvm
              ]
              ++ lib.optionals (system != "x86_64-linux") [
                self'.packages.ukci-x86_64-latest-llvm
                self'.packages.run-qemu-x86_64-latest-llvm
                self'.packages.test-qemu-x86_64-latest-llvm
              ]
              ++ lib.optionals (system != "riscv64-linux") [
                self'.packages.ukci-riscv64-latest-llvm
                self'.packages.run-qemu-riscv64-latest-llvm
                self'.packages.test-qemu-riscv64-latest-llvm
              ];
            };

            treefmt.config = {
              inherit (config.flake-root) projectRootFile;
              # formats .nix files
              programs.nixfmt.enable = true;
            };
          };
      }
    );
}
