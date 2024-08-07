#ifndef __VMLINUX_COMMON_H__
#define __VMLINUX_COMMON_H__

#ifdef __x86_64__
#include "x86_64/vmlinux.h"
#elif __aarch64__
#include "aarch64/vmlinux.h"
#elif __riscv64__
#include "riscv64/vmlinux.h"
#endif

#endif
