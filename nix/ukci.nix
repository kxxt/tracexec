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
          isRiscv64 = system == "riscv64-linux";
          gnutlsOverlay = final: prev: {
            gnutls = prev.gnutls.overrideAttrs (prevAttrs: {
              postPatch = (prevAttrs.postPatch or "") + ''
                touch doc/stamp_error_codes
              '';
            });
          };
          pkgsWithOverlay = pkgs.extend gnutlsOverlay;
          nativeTargetSystems = [ system ];
          crossTargetSystems =
            if isX86_64 then
              [
                "aarch64-linux"
                "riscv64-linux"
              ]
            else if isAarch64 then
              [
                "x86_64-linux"
                "riscv64-linux"
              ]
            else if isRiscv64 then
              [
                "x86_64-linux"
                "aarch64-linux"
              ]
            else
              [ ];
          pkgsForTarget =
            targetSystem:
            if targetSystem == system then
              pkgs
            else if targetSystem == "x86_64-linux" then
              pkgsWithOverlay.pkgsCross.gnu64
            else if targetSystem == "aarch64-linux" then
              pkgsWithOverlay.pkgsCross.aarch64-multiplatform
            else if targetSystem == "riscv64-linux" then
              pkgsWithOverlay.pkgsCross.riscv64
            else
              builtins.abort "Unsupported cross target ${targetSystem} on host ${system}";
          vmSshPort = "10022";
          sourcesFor =
            targetSystem:
            let
              isTargetAarch64 = targetSystem == "aarch64-linux";
              isTargetX86_64 = targetSystem == "x86_64-linux";
              isTargetRiscv64 = targetSystem == "riscv64-linux";
              riscv64BpfLocalStorageFix = {
                # BPF task local storage is broken on RISC-V after 6.19.
                # I have posted this fix to mailing list.
                name = "riscv64-bpf-local-storage-fix";
                patch = ./patches/6.19-riscv64-fix-task-local-storage.patch;
              };
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
                # MSKV for riscv64 (theoretical MSKV is 5.19 but kernel crashed after loading eBPF prog)
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
                kernelPatches = [ riscv64BpfLocalStorageFix ];
                extraMakeFlags = [ ];
              }
              {
                name = "7.0";
                tag = "v7.0-rc2";
                version = "7.0.0-rc2";
                source = "linus";
                test_exe = "tracexec";
                sha256 = "sha256-BlKlJdEYvwDN6iWJfuOvd1gcm6lN6McJ/vmMwOmzHdc=";
                # Same as 6.19
                kernelPatches = [ riscv64BpfLocalStorageFix ];
                extraMakeFlags = [ ];
              }
            ];
          sourcesForTargets =
            targetSystems:
            lib.concatMap (
              targetSystem: map (source: source // { inherit targetSystem; }) (sourcesFor targetSystem)
            ) targetSystems;
          nixpkgs = localFlake.nixpkgs;
          tracexecFor =
            targetPkgs:
            (import ./tracexec-package.nix {
              lib = targetPkgs.lib;
              pkgs = targetPkgs;
              inherit (localFlake) crane;
            })
              { };
          mkKernels =
            targetSystems:
            let
              sources = sourcesForTargets targetSystems;
              useArchSuffix = builtins.length targetSystems > 1;
            in
            map (
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
                kernelName = if useArchSuffix then "${source.name}-${targetArch}" else source.name;
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
                    # bpftrace is very useful for debugging nasty kernel bpf bugs.
                    # Warning: cross-compiling it would take some time so disable it by default.
                    # bpftrace = "${targetPkgs.bpftrace.override {
                    #     bcc = (targetPkgs.bcc.override {
                    #       luajit = null;
                    #     }).overrideAttrs (old: {
                    #       buildInputs = builtins.filter
                    #         (pkg: (pkg.pname or "") != "luajit")
                    #         old.buildInputs;

                    #       nativeBuildInputs = builtins.filter
                    #         (pkg: (pkg.pname or "") != "luajit")
                    #         (old.nativeBuildInputs or []);
                    #     });
                    # }}/bin/bpftrace";
                    nix-store = "${targetPkgs.nix}/bin/nix";
                  };
                  storePaths = [ ];
                };
              }
            ) sources;
          kernelsNative = mkKernels nativeTargetSystems;
          kernelsCross = mkKernels crossTargetSystems;
          targetSystemsAll = nativeTargetSystems ++ crossTargetSystems;
          mkScripts =
            {
              nameSuffix ? "",
              kernels,
            }:
            let
              runQemuName = "run-qemu${nameSuffix}";
              testQemuName = "test-qemu${nameSuffix}";
              ukciName = "ukci${nameSuffix}";
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
              platforms = lib.concatMapStringsSep " " (
                {
                  name,
                  targetSystem,
                  test_exe,
                  testPackage,
                  ...
                }:
                "${name}:${targetSystem}:${test_exe}:${testPackage}"
              ) kernels;
              defaultPackage = if kernels == [ ] then "" else (builtins.head kernels).testPackage;
            in
            let
              runQemuDrv = pkgs.writeScriptBin runQemuName ''
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
                  riscv64)
                    archSpecificArgs=(-machine virt -append "console=ttyS0 earlycon")
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

                echo "Booting $kernel with $initrd in qemu"

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
              testQemuDrv = pkgs.writeScriptBin testQemuName ''
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
                if [ -z "$package" ]; then
                  package="${defaultPackage}"
                fi
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
              ukciDrv = pkgs.writeScriptBin ukciName ''
                #!/usr/bin/env bash

                set -e

                if ! [[ "$(whoami)" == root ]]; then
                  sudo "$0"
                  exit $?
                fi

                ssh="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@127.0.0.1 -p ${vmSshPort}"
                for platform in ${platforms}; do
                  IFS=: read -r kernel target_system test_exe package <<< "$platform"
                  ${runQemuDrv}/bin/${runQemuName} "$kernel" &
                  ${testQemuDrv}/bin/${testQemuName} "$test_exe" "$package"
                  $ssh poweroff -f || true
                  wait < <(jobs -p)
                done;
              '';
            in
            {
              run-qemu = runQemuDrv;
              test-qemu = testQemuDrv;
              ukci = ukciDrv;
            };
        in
        let
          nativeScripts = mkScripts { kernels = kernelsNative; };
          mkTargetScriptAttrs =
            targetSystem:
            let
              arch = getArch targetSystem;
              scripts = mkScripts {
                nameSuffix = "-${arch}";
                kernels = mkKernels [ targetSystem ];
              };
            in
            lib.listToAttrs [
              (lib.nameValuePair "run-qemu-${arch}" scripts.run-qemu)
              (lib.nameValuePair "test-qemu-${arch}" scripts.test-qemu)
              (lib.nameValuePair "ukci-${arch}" scripts.ukci)
            ];
          perTargetScripts = lib.foldl' lib.recursiveUpdate { } (map mkTargetScriptAttrs targetSystemsAll);
        in
        rec {
          inherit (nativeScripts) run-qemu test-qemu ukci;
        }
        // perTargetScripts;
    };
}
