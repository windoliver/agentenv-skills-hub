#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QdrantConfig {
    pub endpoint: String,
}

impl QdrantConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }
}
