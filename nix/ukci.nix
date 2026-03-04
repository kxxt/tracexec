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
          vmSshPort = "10022";
          sources = [
            {
              name = "6.1lts";
              tag = "6.1.165";
              source = "mirror";
              test_exe = "tracexec_no_rcu_kfuncs";
              sha256 = "sha256-WoFxhPG+kBt1x+CvwCN2vDrFFiTgPsb3uKX505XSLvQ=";
            }
            {
              name = "6.6lts";
              tag = "6.6.128";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-ZpYzu4SAAh8Vw4iD+y9v4gh8yLaaWC8m9rfUrms0jkg=";
            }
            {
              name = "6.12lts";
              tag = "6.12.75";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-Bu55J1Vv8aqIEMSCZQGw/bFp69wYBkS4gs98FDrBwXc=";
            }
            {
              name = "6.18lts";
              tag = "6.18.16";
              version = "6.18.16";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-TyHAH00EwdGz7XlBU/iQCALJJJe+YgsHxIaVMPLSjuM=";
            }
            {
              name = "6.19";
              tag = "6.19.6";
              version = "6.19.6";
              source = "mirror";
              test_exe = "tracexec_use_iter_bits";
              sha256 = "sha256-TZ8/9zIU9owBlO8C25ykt7pxMlOsEEVEHU6fNSvCLhQ=";
            }
            {
              name = "7.0";
              tag = "v7.0-rc2";
              version = "7.0.0-rc2";
              source = "linus";
              test_exe = "tracexec_use_iter_bits";
              sha256 = "sha256-BlKlJdEYvwDN6iWJfuOvd1gcm6lN6McJ/vmMwOmzHdc=";
            }
          ];
          nixpkgs = localFlake.nixpkgs;
          configureKernel = pkgs.callPackage ./kernel-configure.nix { };
          buildKernel = pkgs.callPackage ./kernel-build.nix { };
          kernelNixConfig = source: pkgs.callPackage ./kernel-source.nix source;
          kernels = map (
            source:
            let
              config = kernelNixConfig source;
              inherit (config) kernelArgs kernelConfig;
              configfile = configureKernel {
                inherit (kernelConfig)
                  generateConfigFlags
                  structuredExtraConfig
                  ;
                inherit kernel nixpkgs;
              };
              linuxDev = pkgs.linuxPackagesFor kernelDrv;
              kernel = linuxDev.kernel;
              kernelDrv = buildKernel {
                inherit (kernelArgs)
                  src
                  modDirVersion
                  version
                  kernelPatches
                  ;
                inherit configfile nixpkgs;
              };
              buildInitramfs = pkgs.callPackage ./initramfs.nix { };
            in
            {
              inherit kernel;
              inherit (source) name test_exe;
              initramfs = buildInitramfs {
                inherit kernel;
                extraBin = {
                  # We exclude tracexec from it to avoid constant rebuilding of initrds in CI.
                  # tracexec = "${self'.packages.tracexec}/bin/tracexec";
                  # tracexec_no_rcu_kfuncs = "${self'.packages.tracexec_no_rcu_kfuncs}/bin/tracexec";
                  strace = "${pkgs.strace}/bin/strace";
                  nix-store = "${pkgs.nix}/bin/nix";
                };
                storePaths = [ pkgs.foot.terminfo ];
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
                  ...
                }:
                ''
                  ${name})
                    kernel="${kernel}"
                    initrd="${initramfs}"
                  ;;
                ''
              ) kernels;
            in
            pkgs.writeScriptBin "run-qemu" ''
              #!/usr/bin/env sh

              case "$1" in
                ${shellCases}
                *)
                  echo "Invalid argument!"
                  exit 1
              esac

              sudo qemu-system-x86_64 \
                -enable-kvm \
                -m 4G \
                -smp cores=4 \
                -kernel "$kernel"/bzImage \
                -initrd "$initrd"/initrd.gz \
                -device e1000,netdev=net0 \
                -netdev user,id=net0,hostfwd=::${vmSshPort}-:22 \
                -nographic -append "console=ttyS0"
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
            case "$1" in
              tracexec)
                package="${self'.packages.tracexec}"
                ;;
              tracexec_no_rcu_kfuncs)
                package="${self'.packages.tracexec_no_rcu_kfuncs}"
                ;;
              tracexec_use_iter_bits)
                package="${self'.packages.tracexec_use_iter_bits}"
                ;;
              *)
                echo "Unrecognized executable!"
                exit 1
                ;;
            esac
            ${pkgs.nix}/bin/nix copy --to ssh://root@127.0.0.1 "$package"
            $ssh "$package"/bin/tracexec ebpf log -- ls
            exit $?
          '';

          ukci =
            let
              platforms = lib.concatMapStringsSep " " ({ name, test_exe, ... }: "${name}:${test_exe}") kernels;
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
                IFS=: read -r kernel test_exe <<< "$platform"
                ${run-qemu}/bin/run-qemu "$kernel" &
                ${test-qemu}/bin/test-qemu "$test_exe"
                $ssh poweroff -f || true
                wait < <(jobs -p)
              done;
            '';
        };
    };
}
