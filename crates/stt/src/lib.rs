// STT adapter placeholder — implements interfaces for local STT backends
pub fn available_models() -> Vec<&'static str> {
    vec!["tiny", "base", "small", "medium", "large"]
}
