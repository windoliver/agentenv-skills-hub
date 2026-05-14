#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilvusConfig {
    pub endpoint: String,
}

impl MilvusConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }
}
