#!/bin/sh
bindgen --allowlist-var 'AUDIT_ARCH_.*' /usr/include/linux/audit.h > audit.rs
