use hub_core::{
    error::{HubError, HubResult},
    model::{
        CompatibilityIndex, CompatibilitySkillHit, PublishSkillRequest, SkillVersionRecord,
        Visibility,
    },
};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PgHubRepository {
    pool: PgPool,
}

impl PgHubRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_version(
        &self,
        namespace: &str,
        visibility: Visibility,
        published_by: &str,
        request: &PublishSkillRequest,
    ) -> HubResult<SkillVersionRecord> {
        let now = OffsetDateTime::now_utc();
        let skill_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let visibility_text = visibility_text(visibility);
        let manifest_json = serde_json::to_value(&request.manifest).expect("manifest serializes");

        let mut tx = self.pool.begin().await.map_err(db_error)?;
        let existing_skill_id =
            sqlx::query("SELECT id FROM skills WHERE namespace = $1 AND name = $2")
                .bind(namespace)
                .bind(&request.manifest.name)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db_error)?
                .map(|row| row.get::<Uuid, _>("id"));

        let actual_skill_id = if let Some(id) = existing_skill_id {
            id
        } else {
            sqlx::query(
                "INSERT INTO skills (id, namespace, name, description, latest_version, visibility, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $7)",
            )
            .bind(skill_id)
            .bind(namespace)
            .bind(&request.manifest.name)
            .bind(&request.manifest.description)
            .bind(&request.manifest.version)
            .bind(visibility_text)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            skill_id
        };

        let conflict = sqlx::query(
            "SELECT sv.id, s.namespace, s.name, sv.version, s.description, sv.digest,
                    sv.artifact_url, sv.artifact_media_type, sv.signature_ed25519,
                    sv.public_key_ed25519, sv.yanked_at, sv.yank_reason, sv.created_at
             FROM skill_versions sv
             JOIN skills s ON s.id = sv.skill_id
             WHERE sv.skill_id = $1 AND sv.version = $2",
        )
        .bind(actual_skill_id)
        .bind(&request.manifest.version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
        if let Some(row) = conflict {
            let existing: String = row.get("digest");
            if existing != request.artifact.digest {
                return Err(HubError::VersionDigestConflict {
                    namespace: namespace.to_owned(),
                    name: request.manifest.name.clone(),
                    version: request.manifest.version.clone(),
                });
            }
            tx.commit().await.map_err(db_error)?;
            return Ok(skill_version_record(row));
        }

        sqlx::query(
            "INSERT INTO skill_versions
             (id, skill_id, version, digest, manifest_json, artifact_url, artifact_media_type,
              signature_ed25519, public_key_ed25519, sigstore_bundle_json, published_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (skill_id, version) DO NOTHING",
        )
        .bind(version_id)
        .bind(actual_skill_id)
        .bind(&request.manifest.version)
        .bind(&request.artifact.digest)
        .bind(manifest_json)
        .bind(&request.artifact.url)
        .bind(&request.artifact.media_type)
        .bind(&request.signature_ed25519)
        .bind(&request.public_key_ed25519)
        .bind(&request.sigstore_bundle)
        .bind(published_by)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;

        sqlx::query("UPDATE skills SET latest_version = $1, updated_at = $2 WHERE id = $3")
            .bind(&request.manifest.version)
            .bind(now)
            .bind(actual_skill_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;

        tx.commit().await.map_err(db_error)?;

        Ok(SkillVersionRecord {
            id: version_id,
            namespace: namespace.to_owned(),
            name: request.manifest.name.clone(),
            version: request.manifest.version.clone(),
            description: request.manifest.description.clone(),
            digest: request.artifact.digest.clone(),
            artifact_url: request.artifact.url.clone(),
            artifact_media_type: request.artifact.media_type.clone(),
            signature_ed25519: request.signature_ed25519.clone(),
            public_key_ed25519: request.public_key_ed25519.clone(),
            yanked_at: None,
            yank_reason: None,
            created_at: now,
        })
    }

    pub async fn yank_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        reason: &str,
    ) -> HubResult<()> {
        let rows = sqlx::query(
            "UPDATE skill_versions sv
             SET yanked_at = $1, yank_reason = $2
             FROM skills s
             WHERE sv.skill_id = s.id AND s.namespace = $3 AND s.name = $4 AND sv.version = $5",
        )
        .bind(OffsetDateTime::now_utc())
        .bind(reason)
        .bind(namespace)
        .bind(name)
        .bind(version)
        .execute(&self.pool)
        .await
        .map_err(db_error)?
        .rows_affected();
        if rows == 0 {
            return Err(HubError::SkillVersionNotFound {
                namespace: namespace.to_owned(),
                name: name.to_owned(),
                version: version.to_owned(),
            });
        }
        Ok(())
    }

    pub async fn compatibility_index(&self, registry: &str) -> HubResult<CompatibilityIndex> {
        let rows = sqlx::query(
            "SELECT s.name, sv.version, s.description, sv.digest, sv.signature_ed25519, sv.public_key_ed25519
             FROM skills s
             JOIN skill_versions sv ON sv.skill_id = s.id
             WHERE s.visibility = 'public' AND sv.yanked_at IS NULL
             ORDER BY s.name ASC, sv.version ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;

        Ok(CompatibilityIndex {
            skills: rows
                .into_iter()
                .map(|row| CompatibilitySkillHit {
                    name: row.get("name"),
                    version: row.get("version"),
                    description: row.get("description"),
                    registry: registry.to_owned(),
                    digest: Some(row.get("digest")),
                    signature_ed25519: row.get("signature_ed25519"),
                    public_key_ed25519: row.get("public_key_ed25519"),
                })
                .collect(),
        })
    }
}

fn skill_version_record(row: sqlx::postgres::PgRow) -> SkillVersionRecord {
    SkillVersionRecord {
        id: row.get("id"),
        namespace: row.get("namespace"),
        name: row.get("name"),
        version: row.get("version"),
        description: row.get("description"),
        digest: row.get("digest"),
        artifact_url: row.get("artifact_url"),
        artifact_media_type: row.get("artifact_media_type"),
        signature_ed25519: row.get("signature_ed25519"),
        public_key_ed25519: row.get("public_key_ed25519"),
        yanked_at: row.get("yanked_at"),
        yank_reason: row.get("yank_reason"),
        created_at: row.get("created_at"),
    }
}

fn visibility_text(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Private => "private",
        Visibility::Unlisted => "unlisted",
    }
}

fn db_error(source: sqlx::Error) -> HubError {
    HubError::InvalidArtifactUrl {
        value: "database".to_owned(),
        message: source.to_string(),
    }
}
