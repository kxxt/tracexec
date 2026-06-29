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
                tag = "6.1.176";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-qhl3LbpA6XNzVsANBnHN7b4mzIle/wYoaPCh9oiuRPY=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.6lts";
                tag = "6.6.143";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-2s4fjcnA2/XfFPR+MinNYsKY6DBJaBcx7yKfK6dZKTI=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.12lts";
                tag = "6.12.94";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-6ZiiMrlBjbMwHLWEaOKRpPQdargwYCmzDZkfViUdyNI=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
              {
                name = "6.18lts";
                tag = "6.18.37";
                source = "mirror";
                test_exe = "tracexec";
                sha256 = "sha256-qDzSAOZkbbUoZrgwnpE3uekEi2E8vaEM7SuBGq4SUlU=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
            ]
            ++ (lib.optionals (!isTargetRiscv64) [
              {
                name = "7.1";
                tag = "7.1.2";
                version = "7.1.2";
                source = "kernel-org";
                test_exe = "tracexec";
                sha256 = "sha256-NxmMk3J74kfJ+1MJu4bNXklsYeUyLNjE7KlHa7C1iD8=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
            
              {
                name = "7.2";
                tag = "v7.2-rc1";
                version = "7.2.0-rc1";
                source = "torvalds";
                test_exe = "tracexec";
                sha256 = "sha256-tGDnTPoKQoQWiBBjgh72quimpMaYkbmrEPz07fdwzg0=";
                kernelPatches = [ ];
                extraMakeFlags = [ ];
              }
            ])
            ++ (lib.optionals (!isTargetRiscv64) [
              # {
              #   name = "bpf-next";
              #   tag = "bpf-next-7.1";
              #   version = "7.0.0-rc6";
              #   source = "bpf-next";
              #   test_exe = "tracexec";
              #   sha256 = "sha256-z9S4y2YCgsPoInlUTErvgJOj7OSy1c6xP443HXFPc/c=";
              #   kernelPatches = [ ];
              #   extraMakeFlags = [ ];
              # }
            ]);
          sourcesForTargets =
            targetSystems:
            lib.concatMap (
              targetSystem: map (source: source // { inherit targetSystem; }) (sourcesFor targetSystem)
            ) targetSystems;
          inherit (localFlake) nixpkgs;
          llvmVersions = [
            20
            21
            22
          ];
          latestLlvmVersions = [ (lib.last llvmVersions) ];
          ebpfIntegrationRootTestNames = lib.map (lib.removeSuffix ".rs") (
            lib.filter (lib.hasSuffix ".rs") (
              builtins.attrNames (builtins.readDir ../crates/tracexec-backend-ebpf/tests)
            )
          );
          ebpfExpectedRootTestNames = [ "tracexec_backend_ebpf" ] ++ ebpfIntegrationRootTestNames;
          ebpfExpectedRootTestNamesSpaceSep = lib.concatStringsSep " " ebpfExpectedRootTestNames;
          tracexecForClang =
            targetPkgs: llvmVer:
            let
              bpfClang = targetPkgs.buildPackages.${"llvmPackages_${toString llvmVer}"}.clang.cc;
            in
            (import ./tracexec-package.nix {
              inherit (targetPkgs) lib;
              inherit (localFlake) crane;
              pkgs = targetPkgs;
            })
              { inherit bpfClang; };
          tracexecEbpfRootTestsForClang =
            targetPkgs: llvmVer:
            let
              bpfClang = targetPkgs.buildPackages.${"llvmPackages_${toString llvmVer}"}.clang.cc;
              cargoExtraArgs = "--locked --package tracexec-backend-ebpf --no-default-features";
            in
            (import ./tracexec-package.nix {
              inherit (targetPkgs) lib;
              inherit (localFlake) crane;
              pkgs = targetPkgs;
            })
              {
                inherit bpfClang;
                pnameSuffix = "-ebpf-root-tests";
                inherit cargoExtraArgs;
                cargoArtifacts = targetPkgs.runCommand "empty-cargo-target" { } "mkdir -p $out/target";
                doCheck = false;
                doNotPostBuildInstallCargoBinaries = true;
                buildPhaseCargoCommand = ''
                  mkdir -p "$out/target"
                  export CARGO_TARGET_DIR="$out/target"
                  cargoBuildLog="$out/cargo-test-build.jsonl"
                  cargoWithProfile test --no-run --message-format json-render-diagnostics ${cargoExtraArgs} >"$cargoBuildLog"
                '';
                installPhaseCommand = ''
                  mkdir -p "$out/bin"

                  test_executables=$(mktemp test-executablesXXXX)
                  grep '"package_id":"path+file://.*/crates/tracexec-backend-ebpf#' "$out/cargo-test-build.jsonl" \
                    | grep '"profile":{[^}]*"test":true}' \
                    | grep -E '"target":\{"kind":\["(lib|test)"\]' \
                    | sed -n 's/.*"target":{[^}]*"name":"\([^"]*\)".*"executable":"\([^"]*\)".*/\1 \2/p' \
                    > "$test_executables"

                  installed=0
                  while read -r test_name executable; do
                    if [ -z "$test_name" ] || [ -z "$executable" ]; then
                      continue
                    fi
                    if [ ! -x "$executable" ]; then
                      echo "missing test executable for $test_name: $executable" >&2
                      exit 1
                    fi
                    install -Dm755 "$executable" "$out/bin/$test_name"
                    printf '%s\n' "$test_name" >> "$out/tests.list"
                    installed=$((installed + 1))
                  done < "$test_executables"

                  if [ "$installed" -eq 0 ]; then
                    echo "expected at least one eBPF root test binary" >&2
                    exit 1
                  fi

                  sort -o "$out/tests.list" "$out/tests.list"
                  for test_name in ${ebpfExpectedRootTestNamesSpaceSep}; do
                    if ! grep -qx "$test_name" "$out/tests.list"; then
                      echo "expected root test binary $test_name was not installed" >&2
                      exit 1
                    fi
                  done

                  for helper in compat-exec special-fds-exec; do
                    found=0
                    for candidate in "$out"/target/*/release/"$helper" "$out"/target/release/"$helper"; do
                      if [ -f "$candidate" ] && [ -x "$candidate" ]; then
                        found=$((found + 1))
                      fi
                    done
                    if [ "$found" -ne 1 ]; then
                      echo "expected exactly one helper binary for $helper, found $found" >&2
                      exit 1
                    fi
                  done

                  find "$out/target" -type f | while IFS= read -r candidate; do
                    keep=0
                    base="$(basename "$candidate")"
                    case "$base" in
                      compat-exec|special-fds-exec)
                        keep=1
                        ;;
                    esac
                    if [ "$keep" -eq 0 ]; then
                      rm -f "$candidate"
                    fi
                  done

                  find "$out/target" -type d -empty -delete
                '';
              };
          # Build a single shared initramfs per target system (no kernel dependency)
          initramfsForTarget =
            targetSystem:
            let
              targetPkgs = pkgsForTarget targetSystem;
              buildInitramfs = targetPkgs.callPackage ./initramfs.nix { };
            in
            buildInitramfs {
              extraBin = {
                # We exclude tracexec from it to avoid constant rebuilding of initrds in CI.
                # tracexec = "${self'.packages.tracexec}/bin/tracexec";
                # tracexec_no_rcu_kfuncs = "${self'.packages.tracexec_no_rcu_kfuncs}/bin/tracexec";
                strace = "${targetPkgs.strace}/bin/strace";
                nix-store = "${targetPkgs.nix}/bin/nix";
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
              };
              storePaths = [ ];
            };
          mkKernels =
            {
              targetSystems,
              llvmVersions,
            }:
            let
              sources = sourcesForTargets targetSystems;
              useArchSuffix = builtins.length targetSystems > 1;
              # One shared initramfs per target system
              initramfsMap = builtins.listToAttrs (
                map (ts: {
                  name = ts;
                  value = initramfsForTarget ts;
                }) (lib.unique (map (s: s.targetSystem) sources))
              );
              # Build kernel for each source
              buildKernelForSource =
                source:
                let
                  inherit (source) targetSystem;
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
                  inherit (linuxDev) kernel;
                  kernelDrv = buildKernel {
                    inherit (kernelArgs)
                      src
                      modDirVersion
                      version
                      ;
                    inherit (source) kernelPatches extraMakeFlags;
                    inherit configfile nixpkgs;
                  };
                  baseName = if useArchSuffix then "${source.name}-${targetArch}" else source.name;
                in
                {
                  inherit
                    kernel
                    targetSystem
                    targetArch
                    baseName
                    ;
                  inherit (source) test_exe;
                };
              builtKernels = map buildKernelForSource sources;
              # Create test entries for each (kernel, llvmVersion) combination
              mkEntry =
                builtKernel: llvmVer:
                let
                  targetPkgs = pkgsForTarget builtKernel.targetSystem;
                  testPackage = tracexecForClang targetPkgs llvmVer;
                  rootTestsPackage = tracexecEbpfRootTestsForClang targetPkgs llvmVer;
                in
                {
                  inherit (builtKernel)
                    kernel
                    targetSystem
                    targetArch
                    test_exe
                    ;
                  inherit rootTestsPackage testPackage;
                  name = "${builtKernel.baseName}-clang${toString llvmVer}";
                  initramfs = initramfsMap.${builtKernel.targetSystem};
                  xfail = false;
                };
            in
            lib.concatMap (bk: map (mkEntry bk) llvmVersions) builtKernels;
          kernelsNative = mkKernels {
            targetSystems = nativeTargetSystems;
            inherit llvmVersions;
          };
          kernelsNativeLatest = mkKernels {
            targetSystems = nativeTargetSystems;
            llvmVersions = latestLlvmVersions;
          };
          kernelsCross = mkKernels {
            targetSystems = crossTargetSystems;
            inherit llvmVersions;
          };
          kernelsCrossLatest = mkKernels {
            targetSystems = crossTargetSystems;
            llvmVersions = latestLlvmVersions;
          };
          targetSystemsAll = nativeTargetSystems ++ crossTargetSystems;
          mkScripts =
            {
              nameSuffix ? "",
              kernels,
              llvmVersions,
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
                  rootTestsPackage,
                  xfail,
                  ...
                }:
                "${name}:${targetSystem}:${test_exe}:${testPackage}:${rootTestsPackage}:${if xfail then "1" else "0"}"
              ) kernels;
              defaultPackage = if kernels == [ ] then "" else (builtins.head kernels).testPackage;
              ukciSummaryHeader =
                "| Kernel \ Clang | "
                + lib.concatStringsSep " | " (map (v: "clang ${toString v}") llvmVersions)
                + " |";
              ukciSummarySep = "| --- | " + lib.concatStringsSep " | " (map (_: "---") llvmVersions) + " |";
              llvmVersionsSpaceSep = lib.concatStringsSep " " (map toString llvmVersions);
              runQemuDrv = pkgs.writeScriptBin runQemuName ''
                #!/usr/bin/env bash

                case "$1" in
                  ${shellCases}
                  *)
                    echo "Invalid argument!"
                    exit 1
                esac
                port="''${2:-${vmSshPort}}"

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

                use_kvm=0
                for arg in "''${archSpecificArgs[@]}"; do
                  if [ "$arg" = "-enable-kvm" ]; then
                    use_kvm=1
                    break
                  fi
                done

                echo "Booting $kernel with $initrd in qemu"

                qemu_cmd=("${pkgs.qemu}/bin/qemu-system-$arch")
                if [ "$use_kvm" = "1" ] && [ -e /dev/kvm ] && [ ! -w /dev/kvm ]; then
                  qemu_cmd=(sudo "''${qemu_cmd[@]}")
                fi

                "''${qemu_cmd[@]}" \
                  -m 4G \
                  -smp cores=4 \
                  -kernel "$kernel/$kernelImageFile" \
                  -initrd "$initrd"/initrd.gz \
                  -device e1000,netdev=net0 \
                  -netdev user,id=net0,hostfwd=::"$port"-:22 \
                  -nographic \
                  "''${archSpecificArgs[@]}"
              '';
              testQemuDrv = pkgs.writeScriptBin testQemuName ''
                #!/usr/bin/env sh
                test_exe="$1"
                package="$2"
                root_tests_package="$3"
                port="''${4:-${vmSshPort}}"
                poweroff="''${5:-0}"
                ssh="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null root@127.0.0.1 -p $port"
                if [ "$poweroff" = "1" ]; then
                  cleanup() {
                    $ssh poweroff -f >/dev/null 2>&1 || true
                  }
                  trap cleanup EXIT INT TERM
                fi

                # Wait for the qemu virtual machine to start...
                for i in $(seq 1 100); do
                  [ $i -gt 1 ] && sleep 5;
                  $ssh true && break;
                done;

                # Show uname
                $ssh uname -a
                # Copy tracexec
                export NIX_SSHOPTS="-p $port -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
                # Try to load eBPF module:
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
                if [ -z "$root_tests_package" ]; then
                  echo "Missing eBPF root test package path!"
                  exit 1
                fi
                ${pkgs.nix}/bin/nix copy --to ssh://root@127.0.0.1 "$package" "$root_tests_package"
                $ssh "set -e; for test_bin in \"$root_tests_package\"/bin/*; do echo \"running \$(basename \"\$test_bin\")\"; \"\$test_bin\" --ignored --test-threads=1 --nocapture; done"
              '';
              ukciDrv = pkgs.writeScriptBin ukciName ''
                #!/usr/bin/env bash

                set -e

                logs_dir="$(mktemp -d)"
                results_dir="$(mktemp -d)"
                lock_dir="$logs_dir/.lock"
                status=0
                running=0
                if command -v nproc >/dev/null 2>&1; then
                  max_parallel="''${UKCI_MAX_PARALLEL:-$(nproc)}"
                else
                  max_parallel="''${UKCI_MAX_PARALLEL:-$(getconf _NPROCESSORS_ONLN)}"
                fi
                if [ -z "$max_parallel" ] || [ "$max_parallel" -lt 1 ]; then
                  max_parallel=1
                fi
                if [ -t 1 ]; then
                  RED="$(printf '\033[31m')"
                  GREEN="$(printf '\033[32m')"
                  YELLOW="$(printf '\033[33m')"
                  BLUE="$(printf '\033[34m')"
                  BOLD="$(printf '\033[1m')"
                  RESET="$(printf '\033[0m')"
                else
                  RED=""
                  GREEN=""
                  YELLOW=""
                  BLUE=""
                  BOLD=""
                  RESET=""
                fi
                idx=0
                for platform in ${platforms}; do
                  IFS=: read -r kernel target_system test_exe package root_tests_package xfail <<< "$platform"
                  port=$(( ${vmSshPort} + idx ))
                  name="$kernel"
                  qemu_log="$logs_dir/$name.qemu.log"
                  test_log="$logs_dir/$name.test.log"
                  (
                    set +e
                    max_vm_attempts="''${UKCI_VM_TEST_ATTEMPTS:-2}"
                    attempt=1
                    test_status=1
                    while [ "$attempt" -le "$max_vm_attempts" ]; do
                      if [ "$attempt" -gt 1 ]; then
                        echo "''${YELLOW}''${BOLD}Retrying full VM test ''${name} (attempt $attempt/$max_vm_attempts)...''${RESET}" >&2
                        sleep 3
                      fi
                      ${runQemuDrv}/bin/${runQemuName} "$kernel" "$port" >"$qemu_log" 2>&1 &
                      qemu_pid=$!
                      ${pkgs.coreutils}/bin/timeout 1800s \
                        ${testQemuDrv}/bin/${testQemuName} "$test_exe" "$package" "$root_tests_package" "$port" "1" >"$test_log" 2>&1
                      test_status=$?
                      if kill -0 "$qemu_pid" >/dev/null 2>&1; then
                        kill "$qemu_pid" >/dev/null 2>&1 || true
                      fi
                      wait "$qemu_pid" 2>/dev/null || true
                      if [ "$test_status" -eq 0 ]; then
                        break
                      fi
                      if [ "$xfail" = "1" ] && grep -Fq "failed to load: -E2BIG" "$test_log"; then
                        break
                      fi
                      if [ "$attempt" -ge "$max_vm_attempts" ]; then
                        break
                      fi
                      attempt=$((attempt + 1))
                    done
                    while ! mkdir "$lock_dir" 2>/dev/null; do
                      sleep 0.1
                    done
                    echo "''${BLUE}''${BOLD}===> $name (qemu)''${RESET}"
                    cat "$qemu_log" || true
                    echo "''${BLUE}''${BOLD}===> $name (test)''${RESET}"
                    cat "$test_log" || true
                    effective_status="$test_status"
                    if [ "$xfail" = "1" ]; then
                      if [ "$test_status" -eq 0 ]; then
                        result_label="XPASS"
                        effective_status=1
                        echo "''${RED}''${BOLD}XPASS:''${RESET} ''${name} was expected to fail"
                      elif grep -Fq "failed to load: -E2BIG" "$test_log"; then
                        result_label="XFAIL (E2BIG)"
                        effective_status=0
                        echo "''${YELLOW}''${BOLD}XFAIL:''${RESET} ''${name} hit the expected -E2BIG load failure"
                      elif [ "$test_status" -eq 124 ] || [ "$test_status" -eq 137 ]; then
                        result_label="TIMEOUT"
                        echo "''${RED}''${BOLD}FAIL:''${RESET} ''${name} timed out after 1800s"
                      else
                        result_label="FAIL ($test_status)"
                        echo "''${RED}''${BOLD}FAIL:''${RESET} ''${name} exited with unexpected status ''${test_status}"
                      fi
                    elif [ "$test_status" -eq 124 ] || [ "$test_status" -eq 137 ]; then
                      result_label="TIMEOUT"
                      echo "''${RED}''${BOLD}FAIL:''${RESET} ''${name} timed out after 1800s"
                    elif [ "$test_status" -ne 0 ]; then
                      result_label="FAIL ($test_status)"
                      echo "''${RED}''${BOLD}FAIL:''${RESET} ''${name} exited with status ''${test_status}"
                    else
                      result_label="PASS"
                      echo "''${GREEN}''${BOLD}PASS:''${RESET} ''${name}"
                    fi
                    printf '%s\n' "$result_label" > "$results_dir/$name"
                    rmdir "$lock_dir"
                    rm -f "$qemu_log" "$test_log"
                    exit "$effective_status"
                  ) &
                  running=$(( running + 1 ))
                  if [ "$running" -ge "$max_parallel" ]; then
                    if ! wait -n; then
                      status=1
                    fi
                    running=$(( running - 1 ))
                  fi
                  idx=$(( idx + 1 ))
                done;

                while [ "$running" -gt 0 ]; do
                  if ! wait -n; then
                    status=1
                  fi
                  running=$(( running - 1 ))
                done

                if [ "$status" -ne 0 ]; then
                  echo "''${YELLOW}''${BOLD}One or more tests failed.''${RESET}"
                fi

                summary_title="''${UKCI_SUMMARY_TITLE:-UKCI}"
                render_summary_table() {
                  echo "## $summary_title"
                  echo ""
                  echo "${ukciSummaryHeader}"
                  echo "${ukciSummarySep}"
                  declare -A seen_row=
                  row_order=()
                  for platform in ${platforms}; do
                    IFS=: read -r pname _ts _te _pkg _rtp <<< "$platform"
                    row_key="''${pname%-clang*}"
                    if [ -z "''${seen_row[$row_key]+x}" ]; then
                      seen_row[$row_key]=1
                      row_order+=("$row_key")
                    fi
                  done
                  for row_key in "''${row_order[@]}"; do
                    line="| $row_key |"
                    for llvm_ver in ${llvmVersionsSpaceSep}; do
                      cell_name="$row_key-clang$llvm_ver"
                      f="$results_dir/$cell_name"
                      if [ -f "$f" ]; then
                        cell="$(cat "$f")"
                      else
                        cell="—"
                      fi
                      line="$line $cell |"
                    done
                    echo "$line"
                  done
                  echo ""
                }
                set +e
                render_summary_table
                if [ -n "''${GITHUB_STEP_SUMMARY:-}" ]; then
                  render_summary_table >> "$GITHUB_STEP_SUMMARY"
                fi
                set -e

                rm -rf "$logs_dir" "$results_dir"
                exit "$status"
              '';
            in
            {
              run-qemu = runQemuDrv;
              test-qemu = testQemuDrv;
              ukci = ukciDrv;
            };
          nativeScripts = mkScripts {
            kernels = kernelsNative;
            inherit llvmVersions;
          };
          nativeScriptsLatest = mkScripts {
            nameSuffix = "-latest-llvm";
            kernels = kernelsNativeLatest;
            llvmVersions = latestLlvmVersions;
          };
          mkTargetScriptAttrs =
            {
              targetSystem,
              latest ? false,
            }:
            let
              arch = getArch targetSystem;
              attrSuffix = "${arch}" + lib.optionalString latest "-latest-llvm";
              llvmVersionsForScripts = if latest then latestLlvmVersions else llvmVersions;
              scripts = mkScripts {
                nameSuffix = "-${arch}" + lib.optionalString latest "-latest-llvm";
                kernels = mkKernels {
                  targetSystems = [ targetSystem ];
                  llvmVersions = llvmVersionsForScripts;
                };
                llvmVersions = llvmVersionsForScripts;
              };
            in
            lib.listToAttrs [
              (lib.nameValuePair "run-qemu-${attrSuffix}" scripts.run-qemu)
              (lib.nameValuePair "test-qemu-${attrSuffix}" scripts.test-qemu)
              (lib.nameValuePair "ukci-${attrSuffix}" scripts.ukci)
            ];
          perTargetScripts = lib.foldl' lib.recursiveUpdate { } (
            map (targetSystem: mkTargetScriptAttrs { inherit targetSystem; }) targetSystemsAll
          );
          perTargetScriptsLatest = lib.foldl' lib.recursiveUpdate { } (
            map (
              targetSystem:
              mkTargetScriptAttrs {
                inherit targetSystem;
                latest = true;
              }
            ) targetSystemsAll
          );
        in
        rec {
          inherit (nativeScripts) run-qemu test-qemu ukci;
          ukci-latest-llvm = nativeScriptsLatest.ukci;
        }
        // perTargetScripts
        // perTargetScriptsLatest;
    };
}
