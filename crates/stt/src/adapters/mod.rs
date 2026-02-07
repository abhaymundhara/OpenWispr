/// Platform-specific STT adapter implementations

#[cfg(target_os = "macos")]
pub mod mlx;

#[cfg(target_os = "windows")]
pub mod whisper;
