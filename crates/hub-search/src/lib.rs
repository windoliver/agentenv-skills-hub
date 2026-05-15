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

pub fn text_embedding(text: &str) -> [f32; 3] {
    let mut vector = [0.0_f32; 3];
    for token in text.split(|character: char| !character.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let hash = fnv1a(token.as_bytes());
        let slot = (hash % vector.len() as u64) as usize;
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        vector[slot] += sign;
    }

    let magnitude = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if magnitude == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    for value in &mut vector {
        *value /= magnitude;
    }
    vector
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::text_embedding;

    #[test]
    fn text_embedding_is_stable_and_normalized() {
        let embedding = text_embedding("Review code changes");
        assert_eq!(embedding, text_embedding("Review code changes"));
        assert!(embedding.iter().all(|value| value.is_finite()));
        let magnitude = embedding
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        assert!((magnitude - 1.0).abs() < 0.0001);
    }

    #[test]
    fn text_embedding_handles_empty_text() {
        assert_eq!(text_embedding(" ... "), [1.0, 0.0, 0.0]);
    }
}
