#ifndef __VMLINUX_COMMON_H__
#define __VMLINUX_COMMON_H__

#ifdef TRACEXEC_TARGET_X86_64
#include "x86_64/vmlinux.h"
#elif TRACEXEC_TARGET_AARCH64
#include "aarch64/vmlinux.h"
#elif TRACEXEC_TARGET_RISCV64
#include "riscv64/vmlinux.h"
#endif

#endif
