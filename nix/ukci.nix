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
        };
        storePaths = [pkgs.foot.terminfo];
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
  };
}
