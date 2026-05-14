pub mod lexical;
pub mod milvus;
pub mod pgvector;
pub mod qdrant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use lexical::lexical_rank;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDocument {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

#[async_trait]
pub trait SemanticSearch {
    async fn similar(
        &self,
        embedding: &[f32],
        limit: i64,
    ) -> Result<Vec<SearchDocument>, SearchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("embedding must contain at least one dimension")]
    EmptyEmbedding,
    #[error("embedding contains non-finite value at index {index}")]
    NonFiniteEmbedding { index: usize },
}
