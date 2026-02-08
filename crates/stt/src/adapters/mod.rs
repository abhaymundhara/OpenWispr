/// Platform-specific STT adapter implementations

#[cfg(any(target_os = "macos", all(target_os = "windows", feature = "vulkan")))]
pub(crate) mod backend;

#[cfg(target_os = "macos")]
pub mod mlx;

#[cfg(all(target_os = "windows", feature = "vulkan"))]
pub mod whisper;

pub mod fallback;
