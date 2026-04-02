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
  lib,
  buildEnv,
  writeScript,
  makeInitrdNG,
  bash,
  busybox,
  kmod,
  dropbear,
}:
{
  kernel ? null,
  modules ? [ ],
  extraBin ? { },
  extraContent ? { },
  storePaths ? [ ],
  extraInit ? "",
}:
let
  busyboxStatic = busybox.override { enableStatic = true; };

  initrdBinEnv = buildEnv {
    name = "initrd-emergency-env";
    paths = map lib.getBin initrdBin;
    pathsToLink = [
      "/bin"
      "/sbin"
    ];
    postBuild = lib.concatStringsSep "\n" (
      lib.mapAttrsToList (n: v: "ln -s ${v} $out/bin/${n}") extraBin
    );
  };

  moduleEnv = buildEnv {
    name = "initrd-modules";
    paths = modules;
    pathsToLink = [ "/lib/modules/${kernel.modDirVersion}/misc" ];
  };

  content = {
    "/bin" = "${initrdBinEnv}/bin";
    "/sbin" = "${initrdBinEnv}/sbin";
    "/init" = init;
  }
  // (lib.optionalAttrs (kernel != null) {
    "/modules" = "${moduleEnv}/lib/modules/${kernel.modDirVersion}/misc";
  })
  // extraContent;

  initrdBin = [
    bash
    busyboxStatic
    kmod
    dropbear
  ];

  initialRamdisk = makeInitrdNG {
    compressor = "gzip";
    #strip = false;
    contents =
      map (path: {
        source = path;
      }) storePaths
      ++ lib.mapAttrsToList (n: v: {
        source = v;
        target = n;
      }) content;
  };

  init = writeScript "init" ''
    #!/bin/sh

    export PATH=/bin/

    mkdir -p /proc
    mkdir -p /sys
    mkdir -p /dev
    mount -t devtmpfs none /dev
    mkdir -p /dev/pts
    mount -t devpts none /dev/pts
    mount -t proc none /proc
    mount -t sysfs none /sys
    mount -t debugfs debugfs /sys/kernel/debug
    mkdir -p /sys/fs/cgroup
    mount -t cgroup2 cgroup2 /sys/fs/cgroup

    ln -s /proc/self/fd /dev/fd

    mkdir -p /etc/dropbear
    echo /bin/bash > /etc/shells
    cat > /etc/passwd << "EOF"
    root::0:0:root:/root:/bin/bash
    EOF
    passwd -d root

    ifconfig lo 127.0.0.1
    ifconfig eth0 10.0.2.15
    ip route add default via 10.0.2.2

    # Enable eBPF JIT, unfortunately riscv64 does not select ARCH_WANT_DEFAULT_BPF_JIT
    # on 6.1lts kernel, thus we cannot enable CONFIG_BPF_JIT_DEFAULT_ON for it.
    echo 1 > /proc/sys/net/core/bpf_jit_enable

    ${if kernel != null then ''
    mkdir -p /run/booted-system/kernel-modules/lib/modules/${kernel.modDirVersion}/build
    tar -xf /sys/kernel/kheaders.tar.xz -C /run/booted-system/kernel-modules/lib/modules/${kernel.modDirVersion}/build
    '' else ''
    moddir="$(uname -r)"
    mkdir -p "/run/booted-system/kernel-modules/lib/modules/$moddir/build"
    tar -xf /sys/kernel/kheaders.tar.xz -C "/run/booted-system/kernel-modules/lib/modules/$moddir/build"
    ''}

    dropbear -F -R -E -B &

    ${extraInit}

    cat <<!

    Boot took $(cut -d' ' -f1 /proc/uptime) seconds

            _       _     __ _
      /\/\ (_)_ __ (_)   / /(_)_ __  _   ___  __
     /    \| | '_ \| |  / / | | '_ \| | | \ \/ /
    / /\/\ \ | | | | | / /__| | | | | |_| |>  <
    \/    \/_|_| |_|_| \____/_|_| |_|\__,_/_/\_\

    Welcome to mini_linux


    !

    # Get a new session to allow for job control and ctrl-* support
    exec setsid -c /bin/sh
  '';
in
initialRamdisk
