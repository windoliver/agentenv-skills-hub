use crate::{SearchDocument, SearchError, SemanticSearch};
use async_trait::async_trait;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone)]
pub struct PgVectorSearch {
    pool: PgPool,
}

impl PgVectorSearch {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SemanticSearch for PgVectorSearch {
    async fn similar(
        &self,
        embedding: &[f32],
        limit: i64,
    ) -> Result<Vec<SearchDocument>, SearchError> {
        if limit <= 0 {
            return Ok(Vec::new());
        }

        let query_vector = format_query_vector(embedding)?;
        let rows = sqlx::query(
            "SELECT s.namespace, s.name, sv.version, s.description
             FROM skill_embeddings se
             JOIN skills s ON s.id = se.skill_id
             JOIN skill_versions sv ON sv.id = se.version_id
             WHERE s.visibility = 'public' AND sv.yanked_at IS NULL
             ORDER BY se.embedding <=> $1::vector
             LIMIT $2",
        )
        .bind(query_vector)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SearchDocument {
                namespace: row.get("namespace"),
                name: row.get("name"),
                version: row.get("version"),
                description: row.get("description"),
            })
            .collect())
    }
}

fn format_query_vector(embedding: &[f32]) -> Result<String, SearchError> {
    if embedding.is_empty() {
        return Err(SearchError::EmptyEmbedding);
    }

    let mut vector = String::from("[");
    for (index, value) in embedding.iter().enumerate() {
        if !value.is_finite() {
            return Err(SearchError::NonFiniteEmbedding { index });
        }
        if index > 0 {
            vector.push(',');
        }
        vector.push_str(&value.to_string());
    }
    vector.push(']');

    Ok(vector)
}
