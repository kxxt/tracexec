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
              tag = "6.1.143";
              source = "mirror";
              test_exe = "tracexec_no_rcu_kfuncs";
              sha256 = "sha256-4lvaxNd+axB5+Jni7LJMfoUPKV5zXqe1ZUuQDYXAOwo=";
            }
            {
              name = "6.6lts";
              tag = "6.6.96";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-nuRYQ9Q62y4qxfaoyxCjNOOePjOEr4oM44Ojf3giG90=";
            }
            {
              name = "6.12lts";
              tag = "6.12.36";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-ShaK7S3lqBqt2QuisVOGCpjZm/w0ZRk24X8Y5U8Buow=";
            }
            {
              name = "6.15";
              tag = "6.15.5";
              version = "6.15.5";
              source = "mirror";
              test_exe = "tracexec";
              sha256 = "sha256-LKcHk5wUQxIyZJh01DiqWPEbSxJykPpo0WT4vXnGiLU=";
            }
            {
              name = "6.16";
              tag = "v6.16-rc5";
              version = "6.16.0-rc5";
              source = "linus";
              test_exe = "tracexec";
              sha256 = "sha256-4TGbdYDJeCZWCi5fgwbWNT5G3x2Rw9bDW4Xts6/gS2A=";
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
