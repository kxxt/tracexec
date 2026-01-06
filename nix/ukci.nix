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
              tag = "6.1.158";
              source = "mirror";
              test_exe = "tracexec_no_rcu_kfuncs";
              sha256 = "sha256-rQaL/bYE7A9PfeOFyOerlEAIqnikruypT1Mgbmcmv9o=";
            }
            {
              name = "6.6lts";
              tag = "6.6.119";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-PaCbmAu0BMwoeTR5uy1sY2UiZ5IV/6ZaBMiTV1JT5eg=";
            }
            {
              name = "6.12lts";
              tag = "6.12.62";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-E+LGhayPq13Zkt0QVzJVTa5RSu81DCqMdBjnt062LBM=";
            }
            {
              name = "6.18";
              tag = "6.18.1";
              version = "6.18.1";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-0KeL8/DRKqoQrzta3K7VvHZ7W3hwXl74hdXpMLcuJdU=";
            }
            {
              name = "6.19";
              tag = "v6.19-rc1";
              version = "6.19.0-rc1";
              source = "linus";
              test_exe = "tracexec";
              sha256 = "sha256-itUMYlX2BWUMmqeACu8ZaDMR/S2eBhDSIx1UZl9hh9E=";
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
                -m 2G \
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
