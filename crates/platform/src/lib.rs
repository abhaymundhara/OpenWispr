// Platform abstraction crate. Do NOT put OS checks here — keep platform specifics in `platform-*` crates.

pub trait PlatformImpl {
    fn name() -> &'static str;
}

// Minimal placeholder to satisfy builds. Platform-specific crates will implement real behavior.
pub fn platform_name() -> &'static str {
    "generic-platform"
}
