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
          sources = [
            {
              name = "6.1lts";
              tag = "6.1.128";
              source = "mirror";
              test_exe = "tracexec_no_rcu_kfuncs";
              sha256 = "sha256-h01n0xgVcOaaxrM4U/BEjwX8kNTPPkuqrcSpzt58UPM=";
            }
            {
              name = "6.6lts";
              tag = "6.6.77";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-CBCJ388SWu9nqpQOu5lf94m2RhKfXUsjZeHVaFspqE4=";
            }
            {
              name = "6.12lts";
              tag = "6.12.13";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-8+ve6p5VW0z6zkTilnAFb0AkVB5r0iL7z3dsgYl0+7o=";
            }
            {
              name = "6.13.2";
              tag = "6.13.2";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-zfYpgZBru+lwGutzxPn8yAegmEbCiHMWY9YnF+0a5wU=";
            }
            {
              name = "6.14";
              tag = "v6.14-rc2";
              version = "6.14.0-rc2";
              source = "linus";
              test_exe = "tracexec";
              sha256 = "sha256-/wRx3Hlfo810/F8K2AlwEEujdoQ7nzF4ZeI6MbbM9V8=";
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
                  tracexec = "${self'.packages.tracexec}/bin/tracexec";
                  tracexec_no_rcu_kfuncs = "${self'.packages.tracexec_no_rcu_kfuncs}/bin/tracexec";
                  strace = "${pkgs.strace}/bin/strace";
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
                -netdev user,id=net0,hostfwd=::10022-:22 \
                -nographic -append "console=ttyS0"
            '';
          test-qemu = pkgs.writeScriptBin "test-qemu" ''
            #!/usr/bin/env sh
            ssh="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@127.0.0.1 -p 10022"

            # Wait for the qemu virtual machine to start...
            for i in $(seq 1 12); do
              [ $i -gt 1 ] && sleep 5;
              $ssh true && break;
            done;

            # Show uname
            $ssh uname -a
            # Try to load eBPF module:
            $ssh "$1" ebpf log -- ls
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

              ssh="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@127.0.0.1 -p 10022"
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
