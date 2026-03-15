//! Probe for supported kernel features

use std::{
  collections::HashMap,
  env,
};

use cfg_if::cfg_if;
use procfs::ConfigSetting;
use tracing::warn;

pub fn kernel_have_syscall_wrappers(
  #[allow(unused)] kconfig: Option<&HashMap<String, ConfigSetting>>,
) -> bool {
  // arm64 and x86_64 both have syscall wrappers long before 5.17
  cfg_if! {
   if #[cfg(target_arch = "riscv64")] {
      // https://github.com/torvalds/linux/commit/b21cdb9523e5561b97fd534dbb75d132c5c938ff
      kconfig
        .map(|configs| configs.contains_key("CONFIG_ARCH_HAS_SYSCALL_WRAPPER"))
        .unwrap_or_default() ||
        tracexec_core::is_current_kernel_ge((6, 6)).unwrap_or_default()
    } else {
      true
    }
  }
}

pub fn kernel_have_ftrace_with_direct_calls(
  kconfig: Option<&HashMap<String, ConfigSetting>>,
) -> bool {
  // First, check special env `TRACEXEC_USE_KPROBE`
  if env::var("TRACEXEC_USE_KPROBE")
    .inspect_err(|e| warn!("Failed to read env TRACEXEC_USE_KPROBE: {e}"))
    .map(|v| !v.is_empty())
    .unwrap_or_default()
  {
    return false;
  }
  env::var("TRACEXEC_USE_FENTRY")
    .inspect_err(|e| warn!("Failed to read env TRACEXEC_USE_FENTRY: {e}"))
    .map(|v| !v.is_empty())
    .unwrap_or_default() ||
  // Then, we try to read kernel config
  kconfig
    .map(|configs| configs.contains_key("CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS"))
    .unwrap_or_default() ||
  // Finally, we try to decide based on kernel version
  {
    cfg_if! {
      if #[cfg(target_arch = "x86_64")] {
        // We support linux >= 5.17, which all have this feature
        true
      } else if #[cfg(target_arch = "aarch64")] {
        // https://github.com/torvalds/linux/commit/2aa6ac03516d078cf0c35aaa273b5cd11ea9734c
        tracexec_core::is_current_kernel_ge((6, 4)).unwrap_or_default()
      } else if #[cfg(target_arch = "riscv64")] {
        // https://github.com/torvalds/linux/commit/b21cdb9523e5561b97fd534dbb75d132c5c938ff
        tracexec_core::is_current_kernel_ge((6, 16)).unwrap_or_default()
      } else {
          compile_error!("unsupported architecture");
      }
    }
  }
}
