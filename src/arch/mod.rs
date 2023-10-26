use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        pub mod x86_64;
        pub(crate) use x86_64::*;
    } else if #[cfg(target_arch = "aarch64")] {
        pub mod aarch64;
        pub(crate) use aarch64::*;
    } else {
        compile_error!("unsupported architecture");
    }
}
