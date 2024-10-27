localFlake:

{ lib, config, self, inputs, ... }:
{
  perSystem = { self', system, pkgs, ... }: {
    packages.initramfs =
      let
        nixpkgs = localFlake.nixpkgs;
        buildInitramfs = pkgs.callPackage ./initramfs.nix { };
      in
      buildInitramfs {
        kernel = self'.packages.kernel;
        extraBin = {
          tracexec = "${self'.packages.tracexec}/bin/tracexec";
          strace = "${pkgs.strace}/bin/strace";
        };
        storePaths = [ pkgs.foot.terminfo ];
      };

    packages.kernel =
      let
        nixpkgs = localFlake.nixpkgs;
        configureKernel = pkgs.callPackage ./kernel-configure.nix { };
        buildKernel = pkgs.callPackage ./kernel-build.nix { };
        kernelSource = pkgs.callPackage ./kernel-source.nix { enableGdb = false; };
        inherit (kernelSource) kernelArgs kernelConfig;
        linuxDev = pkgs.linuxPackagesFor kernelDrv;
        kernel = linuxDev.kernel;
        configfile = configureKernel {
          inherit
            (kernelConfig)
            generateConfigFlags
            structuredExtraConfig
            ;
          inherit kernel nixpkgs;
        };
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
      in
      kernel;

    packages.run-qemu = pkgs.writeScriptBin "run-qemu" ''
      #!/usr/bin/env sh
      sudo qemu-system-x86_64 \
        -enable-kvm \
        -m 2G \
        -smp cores=4 \
        -kernel ${self'.packages.kernel}/bzImage \
        -initrd ${self'.packages.initramfs}/initrd.gz \
        -device e1000,netdev=net0 \
        -netdev user,id=net0,hostfwd=::10022-:22 \
        -nographic -append "console=ttyS0"
    '';
  };
}
