#ifndef __COMMON_H__
#define __COMMON_H__

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>

// KFuncs

extern void bpf_rcu_read_lock(void) __ksym;
extern void bpf_rcu_read_unlock(void) __ksym;

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

#ifdef __x86_64__
#define SYSCALL_PREFIX "x64"
#define SYSCALL_COMPAT_PREFIX "ia32"
#elif __aarch64__
#define SYSCALL_PREFIX "arm64"
#elif __riscv64__
#define SYSCALL_PREFIX "riscv"
#endif

struct sys_enter_exec_args {
  bool is_execveat;
  bool is_compat;
  const u8 *base_filename;
  const u8 *const *argv;
  const u8 *const *envp;
};

#endif
