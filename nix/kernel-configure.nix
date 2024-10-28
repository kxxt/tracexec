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

{ stdenv
, lib
, perl
, gmp
, libmpc
, mpfr
, bison
, flex
, pahole
, buildPackages
,
}: { nixpkgs
   , kernel
   , # generate-config.pl flags. see below
     generateConfigFlags
   , structuredExtraConfig
   ,
   }:
let
  nativeBuildInputs =
    [ perl gmp libmpc mpfr bison flex pahole ];

  passthru = rec {
    module = import "${nixpkgs}/nixos/modules/system/boot/kernel_config.nix";
    # used also in apache
    # { modules = [ { options = res.options; config = svc.config or svc; } ];
    #   check = false;
    # The result is a set of two attributes
    moduleStructuredConfig =
      (lib.evalModules {
        modules = [
          module
          {
            settings = structuredExtraConfig;
            _file = "structuredExtraConfig";
          }
        ];
      }).config;

    structuredConfig = moduleStructuredConfig.settings;
  };
in
stdenv.mkDerivation (
  {
    kernelArch = stdenv.hostPlatform.linuxArch;
    extraMakeFlags = [ ];

    inherit (kernel) src patches version;
    pname = "linux-config";

    # Flags that get passed to generate-config.pl
    # ignoreConfigErrors: Ignores any config errors in script (eg unused options)
    # autoModules: Build every available module
    # preferBuiltin: Build modules as builtin
    inherit (generateConfigFlags) autoModules preferBuiltin ignoreConfigErrors;
    generateConfig = "${nixpkgs}/pkgs/os-specific/linux/kernel/generate-config.pl";

    kernelConfig = passthru.moduleStructuredConfig.intermediateNixConfig;
    passAsFile = [ "kernelConfig" ];

    depsBuildBuild = [ buildPackages.stdenv.cc ];
    inherit nativeBuildInputs;

    platformName = stdenv.hostPlatform.linux-kernel.name;
    # e.g. "bzImage"
    kernelTarget = stdenv.hostPlatform.linux-kernel.target;

    makeFlags =
      lib.optionals (stdenv.hostPlatform.linux-kernel ? makeFlags) stdenv.hostPlatform.linux-kernel.makeFlags;

    postPatch =
      kernel.postPatch
      + ''
        # Patch kconfig to print "###" after every question so that
        # generate-config.pl from the generic builder can answer them.
        sed -e '/fflush(stdout);/i\printf("###");' -i scripts/kconfig/conf.c
      '';

    preUnpack = kernel.preUnpack or "";

    buildPhase = ''
      export buildRoot="''${buildRoot:-build}"
      export HOSTCC=$CC_FOR_BUILD
      export HOSTCXX=$CXX_FOR_BUILD
      export HOSTAR=$AR_FOR_BUILD
      export HOSTLD=$LD_FOR_BUILD
      # Get a basic config file for later refinement with $generateConfig.
      make $makeFlags \
        -C . O="$buildRoot" allnoconfig \
        HOSTCC=$HOSTCC HOSTCXX=$HOSTCXX HOSTAR=$HOSTAR HOSTLD=$HOSTLD \
        CC=$CC OBJCOPY=$OBJCOPY OBJDUMP=$OBJDUMP READELF=$READELF \
        $makeFlags

      # Create the config file.
      echo "generating kernel configuration..."
      ln -s "$kernelConfigPath" "$buildRoot/kernel-config"
      DEBUG=1 ARCH=$kernelArch KERNEL_CONFIG="$buildRoot/kernel-config" AUTO_MODULES=$autoModules \
        PREFER_BUILTIN=$preferBuiltin BUILD_ROOT="$buildRoot" SRC=. MAKE_FLAGS="$makeFlags" \
        perl -w $generateConfig
    '';

    installPhase = "mv $buildRoot/.config $out";

    enableParallelBuilding = true;

    inherit passthru;
  }
)
