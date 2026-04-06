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

pub fn can_i_use_sleepable_fentry(kconfig: Option<&HashMap<String, ConfigSetting>>) -> bool {
  if env::var("TRACEXEC_NO_SLEEP")
    .map(|v| !v.is_empty())
    .unwrap_or_default()
  {
    return false;
  }
  kconfig
    .map(|configs| configs.contains_key("CONFIG_FUNCTION_ERROR_INJECTION"))
    // Defaults to true
    .unwrap_or(true)
}

#[cfg(test)]
mod tests {
  use std::{
    collections::HashMap,
    env,
  };

  use procfs::ConfigSetting;
  use rusty_fork::rusty_fork_test;

  use super::{
    can_i_use_sleepable_fentry,
    kernel_have_ftrace_with_direct_calls,
    kernel_have_syscall_wrappers,
  };

  rusty_fork_test! {
    #[test]
    fn test_can_i_use_sleepable_fentry_env_no_sleep_disables() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::set_var("TRACEXEC_NO_SLEEP", "1");
      }
      let mut configs = HashMap::new();
      configs.insert(
        "CONFIG_FUNCTION_ERROR_INJECTION".to_string(),
        ConfigSetting::Yes,
      );
      assert!(!can_i_use_sleepable_fentry(Some(&configs)));
    }

    #[test]
    fn test_can_i_use_sleepable_fentry_kconfig_has_error_injection() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::remove_var("TRACEXEC_NO_SLEEP");
      }
      let mut configs = HashMap::new();
      configs.insert(
        "CONFIG_FUNCTION_ERROR_INJECTION".to_string(),
        ConfigSetting::Yes,
      );
      assert!(can_i_use_sleepable_fentry(Some(&configs)));
    }

    #[test]
    fn test_can_i_use_sleepable_fentry_kconfig_missing_error_injection() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::remove_var("TRACEXEC_NO_SLEEP");
      }
      let configs = HashMap::new();
      assert!(!can_i_use_sleepable_fentry(Some(&configs)));
    }

    #[test]
    fn test_can_i_use_sleepable_fentry_no_kconfig_defaults_true() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::remove_var("TRACEXEC_NO_SLEEP");
      }
      assert!(can_i_use_sleepable_fentry(None));
    }

    #[test]
    fn test_can_i_use_sleepable_fentry_empty_env_does_not_disable() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::set_var("TRACEXEC_NO_SLEEP", "");
      }
      assert!(can_i_use_sleepable_fentry(None));
    }

    #[test]
    fn test_kernel_have_ftrace_direct_calls_env_force_kprobe() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::set_var("TRACEXEC_USE_KPROBE", "1");
        env::remove_var("TRACEXEC_USE_FENTRY");
      }
      assert!(!kernel_have_ftrace_with_direct_calls(None));
    }

    #[test]
    fn test_kernel_have_ftrace_direct_calls_env_forces_fentry() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::set_var("TRACEXEC_USE_FENTRY", "1");
        env::remove_var("TRACEXEC_USE_KPROBE");
      }
      assert!(kernel_have_ftrace_with_direct_calls(None));
    }

    #[test]
    fn test_kernel_have_ftrace_kconfig_supports_direct_calls() {
      // SAFETY: we do this in a separate subprocess.
      unsafe {
        env::remove_var("TRACEXEC_USE_KPROBE");
        env::remove_var("TRACEXEC_USE_FENTRY");
      }
      let mut configs = HashMap::new();
      configs.insert(
        "CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS".to_string(),
        ConfigSetting::Yes,
      );
      assert!(kernel_have_ftrace_with_direct_calls(Some(&configs)));
    }
  }

  #[cfg(target_arch = "riscv64")]
  #[test]
  fn test_kernel_have_syscall_wrappers_with_kconfig_on_riscv64() {
    let mut configs = HashMap::new();
    configs.insert(
      "CONFIG_ARCH_HAS_SYSCALL_WRAPPER".to_string(),
      ConfigSetting::Yes,
    );
    assert!(kernel_have_syscall_wrappers(Some(&configs)));
  }

  #[cfg(not(target_arch = "riscv64"))]
  #[test]
  fn test_kernel_have_syscall_wrappers_on_non_riscv64() {
    assert!(kernel_have_syscall_wrappers(None));
  }
}
