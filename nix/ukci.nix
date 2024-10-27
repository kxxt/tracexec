localFlake:

{ lib, config, self, inputs, ... }:
{
  perSystem = { self', system, pkgs, ... }: {
    packages =
      let
        sources = [{ name = "6.6lts"; version = "6.6.58"; sha256 = "sha256-59+B5YjXD6tew+w7sErFPVHwhg/DsexF4KQWegJomds="; }
          { name = "6.1lts"; version = "6.1.113"; sha256 = "sha256-VK8QhxkvzFJQpCUUUf1hR2GFnT2WREncAjWOVYxEnjA="; }
          { name = "6.11"; version = "6.11.5"; sha256 = "sha256-RxSFs7fy+2N72P49AJRMTBNcfY7gLzV/M2kLqrB1Kgc="; }];
        nixpkgs = localFlake.nixpkgs;
        configureKernel = pkgs.callPackage ./kernel-configure.nix { };
        buildKernel = pkgs.callPackage ./kernel-build.nix { };
        kernelNixConfig = source: pkgs.callPackage ./kernel-source.nix { enableGdb = false; inherit (source) version sha256; };
        kernels = map
          (source:
            let
              config = kernelNixConfig source;
              inherit (config) kernelArgs kernelConfig;
              configfile = configureKernel {
                inherit
                  (kernelConfig)
                  generateConfigFlags
                  structuredExtraConfig
                  ;
                inherit kernel nixpkgs;
              };
              linuxDev = pkgs.linuxPackagesFor kernelDrv;
              kernel = linuxDev.kernel;
              kernelDrv = buildKernel {
                inherit
                  (kernelArgs)
                  src
                  modDirVersion
                  version
                  enableGdb
                  kernelPatches
                  ;
                inherit configfile nixpkgs;
              };
              buildInitramfs = pkgs.callPackage ./initramfs.nix { };
            in
            {
              inherit kernel;
              inherit (source) name;
              initramfs = buildInitramfs {
                inherit kernel;
                extraBin = {
                  tracexec = "${self'.packages.tracexec}/bin/tracexec";
                  strace = "${pkgs.strace}/bin/strace";
                };
                storePaths = [ pkgs.foot.terminfo ];
              };
            })
          sources;
      in
      {
        run-qemu =
          let
            shellCases = lib.concatMapStrings
              ({ name, kernel, initramfs }: ''
                ${name})
                  kernel="${kernel}"
                  initrd="${initramfs}"
                ;;
              '')
              kernels;
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

          # Try to load eBPF module:
          $ssh tracexec ebpf log -- ls
          exit $?
        '';
      };
  };
}
