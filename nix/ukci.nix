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
    {
      self',
      system,
      pkgs,
      ...
    }:
    {
      packages =
        let
          getArch =
            systemString:
            let
              split = lib.strings.splitString "-" systemString;
            in
            if builtins.length split != 2 then
              builtins.abort "Invalid system type ${systemString}"
            else
              builtins.elemAt split 0;
          isAarch64 = system == "aarch64-linux";
          isX86_64 = system == "x86_64-linux";
          targetSystems =
            if isX86_64 then
              [
                "x86_64-linux"
                "aarch64-linux"
              ]
            else
              [ system ];
          pkgsForTarget =
            targetSystem:
            if targetSystem == system then
              pkgs
            else if targetSystem == "x86_64-linux" then
              pkgsWithOverlay.pkgsCross.gnu64
            else if targetSystem == "aarch64-linux" then
              pkgs.pkgsCross.aarch64-multiplatform
            else
              builtins.abort "Unsupported cross target ${targetSystem} on host ${system}";
          vmSshPort = "10022";
          sourcesFor =
            targetSystem:
            let
              isTargetAarch64 = targetSystem == "aarch64-linux";
              isTargetX86_64 = targetSystem == "x86_64-linux";
            in
            (lib.optionals isTargetX86_64 [
              {
                # MSKV for x86_64
                name = "5.17";
                tag = "5.17.15";
                source = "mirror-v5";
                test_exe = "tracexec";
                sha256 = "sha256-ShySKkkO6r9bRNT9423pultxcRtzUsYlhxbaQRYNtig=";
                kernelPatches = [
                  {
                    name = "pahole-compatibility-fix";
                    patch = ./patches/5.17-Replace-scripts-pahole-flags.sh-with-the-one-in-5.15.patch;
                  }
                ];
                extraMakeFlags = [ ];
              }
            ])
            ++ (lib.optionals isTargetAarch64 [
              {
                # MSKV for aarch64
                name = "5.18";
                tag = "5.18.19";
                source = "mirror-v5";
                test_exe = "tracexec";
                sha256 = "sha256-3/CbJRcS+zs4fLTg97CXwO88e263+UqMmu5swCP8iNU=";
                kernelPatches = [
                  {
                    name = "pahole-compatibility-fix";
                    patch = ./patches/5.18-Replace-scripts-pahole-flags.sh-with-the-one-in.patch;
                  }
                ];
                extraMakeFlags = [ ];
              }
            ])
            ++ [
              {
                name = "6.1lts";
                tag = "6.1.165";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-WoFxhPG+kBt1x+CvwCN2vDrFFiTgPsb3uKX505XSLvQ=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.6lts";
                tag = "6.6.128";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-ZpYzu4SAAh8Vw4iD+y9v4gh8yLaaWC8m9rfUrms0jkg=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.12lts";
                tag = "6.12.75";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-Bu55J1Vv8aqIEMSCZQGw/bFp69wYBkS4gs98FDrBwXc=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.18lts";
                tag = "6.18.16";
                version = "6.18.16";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-TyHAH00EwdGz7XlBU/iQCALJJJe+YgsHxIaVMPLSjuM=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.19";
                tag = "6.19.6";
                version = "6.19.6";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-TZ8/9zIU9owBlO8C25ykt7pxMlOsEEVEHU6fNSvCLhQ=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "7.0";
                tag = "v7.0-rc2";
                version = "7.0.0-rc2";
                source = "linus";
                test_exe = "tracexec";
                sha256 = "sha256-BlKlJdEYvwDN6iWJfuOvd1gcm6lN6McJ/vmMwOmzHdc=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
            ];
          sources =
            lib.concatMap (
              targetSystem:
              map (source: source // { inherit targetSystem; }) (sourcesFor targetSystem)
            ) targetSystems;
          nixpkgs = localFlake.nixpkgs;
          tracexecFor =
            targetPkgs:
            (import ./tracexec-package.nix {
              lib = targetPkgs.lib;
              pkgs = targetPkgs;
              inherit (localFlake) crane;
            }) { };
          kernels = map (
            source:
            let
              targetSystem = source.targetSystem;
              targetPkgs = pkgsForTarget targetSystem;
              targetArch = getArch targetSystem;
              kernelNixConfig = s: targetPkgs.callPackage ./kernel-source.nix s;
              configureKernel = targetPkgs.callPackage ./kernel-configure.nix { };
              buildKernel = targetPkgs.callPackage ./kernel-build.nix { stdenv = targetPkgs.gcc14Stdenv; };
              config = kernelNixConfig source;
              inherit (config) kernelArgs kernelConfig;
              configfile = configureKernel {
                inherit (kernelConfig)
                  generateConfigFlags
                  structuredExtraConfig
                  ;
                inherit kernel nixpkgs;
              };
              linuxDev = targetPkgs.linuxPackagesFor kernelDrv;
              kernel = linuxDev.kernel;
              kernelDrv = buildKernel {
                inherit (kernelArgs)
                  src
                  modDirVersion
                  version
                  ;
                inherit (source) kernelPatches extraMakeFlags;
                inherit configfile nixpkgs;
              };
              buildInitramfs = targetPkgs.callPackage ./initramfs.nix { };
              useArchSuffix = builtins.length targetSystems > 1;
              kernelName =
                if useArchSuffix then
                  "${source.name}-${targetArch}"
                else
                  source.name;
              testPackage = tracexecFor targetPkgs;
            in
            {
              inherit kernel;
              inherit targetSystem targetArch testPackage;
              name = kernelName;
              inherit (source) test_exe;
              initramfs = buildInitramfs {
                inherit kernel;
                extraBin = {
                  # We exclude tracexec from it to avoid constant rebuilding of initrds in CI.
                  # tracexec = "${self'.packages.tracexec}/bin/tracexec";
                  # tracexec_no_rcu_kfuncs = "${self'.packages.tracexec_no_rcu_kfuncs}/bin/tracexec";
                  strace = "${targetPkgs.strace}/bin/strace";
                  nix-store = "${targetPkgs.nix}/bin/nix";
                };
                storePaths = [ targetPkgs.foot.terminfo ];
              };
            }
          ) sources;
        in
        rec {
          run-qemu =
            let
              shellCases = lib.concatMapStrings (
                {
                  name,
                  kernel,
                  initramfs,
                  targetArch,
                  ...
                }:
                ''
                  ${name})
                    kernel="${kernel}"
                    initrd="${initramfs}"
                    arch="${targetArch}"
                  ;;
                ''
              ) kernels;
            in
            pkgs.writeScriptBin "run-qemu" ''
              #!/usr/bin/env bash

              case "$1" in
                ${shellCases}
                *)
                  echo "Invalid argument!"
                  exit 1
              esac

              case "$arch" in
                aarch64)
                  archSpecificArgs=(-machine virt -cpu neoverse-n2 -append "console=ttyAMA0")
                  kernelImageFile="Image"
                  ;;
                x86_64)
                  archSpecificArgs=(-enable-kvm -append "console=ttyS0")
                  kernelImageFile="bzImage"
                  ;;
                *)
                  archSpecificArgs=()
                  kernelImageFile="Image"
                  ;;
              esac

              sudo ${pkgs.qemu}/bin/qemu-system-$arch \
                -m 4G \
                -smp cores=4 \
                -kernel "$kernel/$kernelImageFile" \
                -initrd "$initrd"/initrd.gz \
                -device e1000,netdev=net0 \
                -netdev user,id=net0,hostfwd=::${vmSshPort}-:22 \
                -nographic \
                "''${archSpecificArgs[@]}"
            '';
          test-qemu = pkgs.writeScriptBin "test-qemu" ''
            #!/usr/bin/env sh
            ssh="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@127.0.0.1 -p ${vmSshPort}"

            # Wait for the qemu virtual machine to start...
            for i in $(seq 1 12); do
              [ $i -gt 1 ] && sleep 5;
              $ssh true && break;
            done;

            # Show uname
            $ssh uname -a
            # Copy tracexec
            export NIX_SSHOPTS="-p ${vmSshPort} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
            # Try to load eBPF module:
            test_exe="$1"
            package="$2"
            case "$test_exe" in
              tracexec) : ;;
              *)
                echo "Unrecognized executable!"
                exit 1
                ;;
            esac
            if [ -z "$package" ]; then
              echo "Missing package path!"
              exit 1
            fi
            ${pkgs.nix}/bin/nix copy --to ssh://root@127.0.0.1 "$package"
            $ssh "$package"/bin/tracexec ebpf log -- ls
            exit $?
          '';

          ukci =
            let
              platforms = lib.concatMapStringsSep " " (
                { name, targetSystem, test_exe, testPackage, ... }:
                "${name}:${targetSystem}:${test_exe}:${testPackage}"
              ) kernels;
            in
            pkgs.writeScriptBin "ukci" ''
              #!/usr/bin/env bash

              set -e

              if ! [[ "$(whoami)" == root ]]; then
                sudo "$0"
                exit $?
              fi

              ssh="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@127.0.0.1 -p ${vmSshPort}"
              for platform in ${platforms}; do
                IFS=: read -r kernel target_system test_exe package <<< "$platform"
                ${run-qemu}/bin/run-qemu "$kernel" &
                ${test-qemu}/bin/test-qemu "$test_exe" "$package"
                $ssh poweroff -f || true
                wait < <(jobs -p)
              done;
            '';
        };
    };
}
