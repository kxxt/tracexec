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

{ pkgs
, lib ? pkgs.lib
, enableGdb
, version
, sha256
}:
let
  localVersion = "-ukci";
in
{
  kernelArgs = {
    inherit enableGdb;

    inherit version;
    src =
      pkgs.fetchurl {
        url = "mirror://kernel/linux/kernel/v6.x/linux-${version}.tar.xz";
        inherit sha256;
      };

    # Add kernel patches here
    kernelPatches =
      let
        fetchSet = lib.imap1 (i: hash: {
          # name = " "kbuild-v${builtins.toString i}";
          patch = pkgs.fetchpatch {
            inherit hash;
            url = "https://lore.kernel.org/rust-for-linux/20230109204520.539080-${builtins.toString i}-ojeda@kernel.org/raw";
          };
        });

        patches = fetchSet [
        ];
      in
      patches;

    inherit localVersion;
    modDirVersion = version + localVersion;
  };
  kernelConfig = {
    # See https://github.com/NixOS/nixpkgs/blob/master/nixos/modules/system/boot/kernel_config.nix
    structuredExtraConfig = with lib.kernel;
      {
        DEBUG_FS = yes;
        DEBUG_KERNEL = yes;
        DEBUG_MISC = yes;
        DEBUG_BUGVERBOSE = yes;
        DEBUG_BOOT_PARAMS = yes;
        DEBUG_STACK_USAGE = yes;
        DEBUG_SHIRQ = yes;
        DEBUG_ATOMIC_SLEEP = yes;

        IKCONFIG = yes;
        IKCONFIG_PROC = yes;
        # Compile with headers
        IKHEADERS = yes;

        SLUB_DEBUG = yes;
        DEBUG_MEMORY_INIT = yes;
        KASAN = yes;

        # FRAME_WARN - warn at build time for stack frames larger tahn this.

        MAGIC_SYSRQ = yes;

        LOCALVERSION = freeform localVersion;

        LOCK_STAT = yes;
        PROVE_LOCKING = yes;

        FTRACE = yes;
        STACKTRACE = yes;
        IRQSOFF_TRACER = yes;

        KGDB = yes;
        UBSAN = yes;
        BUG_ON_DATA_CORRUPTION = yes;
        SCHED_STACK_END_CHECK = yes;
        UNWINDER_FRAME_POINTER = yes;
        "64BIT" = yes;
        SMP = yes;
        IA32_EMULATION = yes;
        PREEMPT_DYNAMIC = yes;

        # initramfs/initrd support
        BLK_DEV_INITRD = yes;

        PRINTK = yes;
        PRINTK_TIME = yes;
        EARLY_PRINTK = yes;

        # Support elf and #! scripts
        BINFMT_ELF = yes;
        BINFMT_SCRIPT = yes;

        # Create a tmpfs/ramfs early at bootup.
        DEVTMPFS = yes;
        DEVTMPFS_MOUNT = yes;

        TTY = yes;
        SERIAL_8250 = yes;
        SERIAL_8250_CONSOLE = yes;

        PROC_FS = yes;
        SYSFS = yes;

        MODULES = yes;
        MODULE_UNLOAD = yes;

        # FW_LOADER = yes;

        ##
        SYSVIPC = yes;
        NET = yes;
        PACKET = yes;
        INET = yes;
        NETDEVICES = yes;
        NET_CORE = yes;
        ETHERNET = yes;
        PCI = yes;
        NET_VENDOR_INTEL = yes;
        E1000 = yes;
        UNIX = yes;

        EXPERT = yes;
        TMPFS = yes;
        MEMFD_CREATE = yes;

        PID_NS = yes;

        SECCOMP = yes;

        ## BPF
        DEBUG_INFO_DWARF4 = yes;
        DEBUG_INFO_BTF = yes;
        BPF_SYSCALL = yes;
        BPF_JIT = yes;
        FUNCTION_TRACER = yes;
        # Enable kprobes and kallsyms: https://www.kernel.org/doc/html/latest/trace/kprobes.html#configuring-kprobes
        # Debug FS is be enabled (done above) to show registered kprobes in /sys/kernel/debug: https://www.kernel.org/doc/html/latest/trace/kprobes.html#the-kprobes-debugfs-interface
        KPROBES = yes;
        KALLSYMS_ALL = yes;
      } // lib.optionalAttrs enableGdb {
        DEBUG_INFO_DWARF_TOOLCHAIN_DEFAULT = yes;
        GDB_SCRIPTS = yes;
      };

    # Flags that get passed to generate-config.pl
    generateConfigFlags = {
      # Ignores any config errors (eg unused config options)
      ignoreConfigErrors = false;
      # Build every available module
      autoModules = false;
      preferBuiltin = false;
    };
  };
}
