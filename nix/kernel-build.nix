# Credits: https://github.com/jordanisaacs/kernel-module-flake
# Original Copyright Notice:

# MIT License

# Copyright (c) 2022 Jordan Isaacs

# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:

# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.

# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

{
  stdenv,
  lib,
  callPackage,
  buildPackages,
  rustPlatform,
}:
{
  src,
  configfile,
  modDirVersion,
  version,
  # Install the GDB scripts
  kernelPatches ? [ ],
  extraMakeFlags,
  nixpkgs, # Nixpkgs source
}:
let
  kernel =
    ((callPackage "${nixpkgs}/pkgs/os-specific/linux/kernel/build.nix" { }) {
      inherit
        src
        modDirVersion
        version
        kernelPatches
        configfile
        extraMakeFlags
        ;
      inherit lib stdenv;

      # Because allowedImportFromDerivation is not enabled,
      # the function cannot set anything based on the configfile. These settings do not
      # actually change the .config but let the kernel derivation know what can be built.
      # See manual-config.nix for other options
      config = {
        # Enables the dev build
        CONFIG_MODULES = "y";
      };
    }).overrideAttrs
      (old: {
        nativeBuildInputs = old.nativeBuildInputs;

        dontStrip = true;

        postInstall = ''
          mkdir -p $dev
          cp vmlinux $dev/

          mkdir -p $dev/lib/modules/${modDirVersion}/{build,source}
          cp -rL $buildRoot/scripts $dev/lib/modules/${modDirVersion}/build/
          cp -L $buildRoot/vmlinux-gdb.py $dev/lib/modules/${modDirVersion}/build/scripts/gdb/
          ln -sfn $dev/lib/modules/${modDirVersion}/build/scripts/gdb/vmlinux-gdb.py $dev/lib/modules/${modDirVersion}/build/vmlinux-gdb.py

          if [ -z "''${dontStrip-}" ]; then
            installFlags+=("INSTALL_MOD_STRIP=1")
          fi
          make modules_install "''${makeFlags[@]}" "''${installFlags[@]}"
          unlink $modules/lib/modules/${modDirVersion}/build

          # To save space, exclude a bunch of unneeded stuff when copying.
          (cd .. && rsync --archive --prune-empty-dirs \
              --exclude='/build/' \
              * $dev/lib/modules/${modDirVersion}/source/)

          cd $dev/lib/modules/${modDirVersion}/source
          cp $buildRoot/{.config,Module.symvers} $dev/lib/modules/${modDirVersion}/build

          make modules_prepare "''${makeFlags[@]}" O=$dev/lib/modules/${modDirVersion}/build

          # For reproducibility, removes accidental leftovers from a `cc1` call
          # from a `try-run` call from the Makefile
          rm -f $dev/lib/modules/${modDirVersion}/build/.[0-9]*.d

          # Keep some extra files on some arches (powerpc, aarch64)
          for f in arch/powerpc/lib/crtsavres.o arch/arm64/kernel/ftrace-mod.o; do
            if [ -f "$buildRoot/$f" ]; then
              cp $buildRoot/$f $dev/lib/modules/${modDirVersion}/build/$f
            fi
          done

          # Not doing the nix default of removing files from the source tree.
          # This is because the source tree is necessary for debugging with GDB.
        '';
      });

  kernelPassthru = {
    inherit (configfile) structuredConfig;
    inherit modDirVersion configfile;
    passthru = kernel.passthru // (removeAttrs kernelPassthru [ "passthru" ]);
  };
in
lib.extendDerivation true kernelPassthru kernel
