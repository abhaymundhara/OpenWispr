/// Platform-specific STT adapter implementations

pub(crate) mod backend;

#[cfg(target_os = "macos")]
pub mod mlx;

#[cfg(target_os = "windows")]
pub mod whisper;
