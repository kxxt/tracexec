#ifndef __COMMON_H__
#define __COMMON_H__

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>


// Macros

#define AT_FDCWD -100
// https://elixir.bootlin.com/linux/v6.10.3/source/include/uapi/asm-generic/fcntl.h#L63
#define O_CLOEXEC 02000000

 #define max(a,b) \
   ({ __typeof__ (a) _a = (a); \
       __typeof__ (b) _b = (b); \
     _a > _b ? _a : _b; })
#define min(a,b) \
   ({ __typeof__ (a) _a = (a); \
       __typeof__ (b) _b = (b); \
     _a > _b ? _b : _a; })

// Ref: https://elixir.bootlin.com/linux/v6.10.3/source/include/uapi/linux/bits.h#L7
#define __AC(X,Y)	(X##Y)
#define _AC(X,Y)	__AC(X,Y)
#define _UL(x)		(_AC(x, UL))
#define GENMASK(h, l) \
        (((~_UL(0)) - (_UL(1) << (l)) + 1) & \
         (~_UL(0) >> (BITS_PER_LONG - 1 - (h))))

// Architecture Specific Definitions

#ifdef TRACEXEC_TARGET_X86_64
#define SYSCALL_PREFIX "x64"
#define SYSCALL_COMPAT_PREFIX "ia32_compat"
#elif TRACEXEC_TARGET_AARCH64
#define SYSCALL_PREFIX "arm64"
#elif TRACEXEC_TARGET_RISCV64
#define SYSCALL_PREFIX "riscv"
#endif

#ifdef TRACEXEC_TARGET_X86_64

#define COMPAT_PT_REGS_PARM1_CORE(x) ((u32)(BPF_CORE_READ(__PT_REGS_CAST(x), bx)))
#define COMPAT_PT_REGS_PARM2_CORE(x) ((u32)(BPF_CORE_READ(__PT_REGS_CAST(x), cx)))
#define COMPAT_PT_REGS_PARM3_CORE(x) ((u32)(BPF_CORE_READ(__PT_REGS_CAST(x), dx)))
#define COMPAT_PT_REGS_PARM4_CORE(x) ((u32)(BPF_CORE_READ(__PT_REGS_CAST(x), si)))
#define COMPAT_PT_REGS_PARM5_CORE(x) ((u32)(BPF_CORE_READ(__PT_REGS_CAST(x), di)))

#endif

// Internal structs

struct sys_enter_exec_args {
  bool is_execveat;
  bool is_compat;
  const u8 *base_filename;
  const u8 *const *argv;
  const u8 *const *envp;
};

// Compatibility Shims

#ifndef NO_RCU_KFUNCS
extern void bpf_rcu_read_lock(void) __ksym;
extern void bpf_rcu_read_unlock(void) __ksym;
#endif

int __always_inline rcu_read_lock() {
#ifndef NO_RCU_KFUNCS
  bpf_rcu_read_lock();
#endif
  return 0;
}

int __always_inline rcu_read_unlock() {
#ifndef NO_RCU_KFUNCS
  bpf_rcu_read_unlock();
#endif
  return 0;
}

#endif
