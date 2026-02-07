// LLM adapter placeholder — local + remote adapters should implement this
pub enum AdapterKind {
    Local,
    Remote,
}

pub fn supported_adapters() -> Vec<AdapterKind> {
    vec![AdapterKind::Local, AdapterKind::Remote]
}
