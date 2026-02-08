/// Platform-specific STT adapter implementations

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) mod backend;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) mod sherpa;

#[cfg(target_os = "macos")]
pub(crate) mod mlx_parakeet;

#[cfg(target_os = "macos")]
pub mod mlx;

#[cfg(target_os = "windows")]
pub mod whisper;
