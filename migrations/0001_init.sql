CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS skills (
    id uuid PRIMARY KEY,
    namespace text NOT NULL,
    name text NOT NULL,
    description text,
    latest_version text,
    visibility text NOT NULL CHECK (visibility IN ('public', 'private', 'unlisted')),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (namespace, name)
);

CREATE TABLE IF NOT EXISTS skill_versions (
    id uuid PRIMARY KEY,
    skill_id uuid NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    version text NOT NULL,
    digest text NOT NULL,
    manifest_json jsonb NOT NULL,
    artifact_url text NOT NULL,
    artifact_media_type text NOT NULL,
    artifact_digest text NOT NULL,
    signature_ed25519 text,
    public_key_ed25519 text,
    sigstore_bundle_json jsonb,
    yanked_at timestamptz,
    yank_reason text,
    published_by text NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (skill_id, version)
);

CREATE TABLE IF NOT EXISTS skill_embeddings (
    skill_id uuid NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    version_id uuid NOT NULL REFERENCES skill_versions(id) ON DELETE CASCADE,
    embedding vector(3),
    embedding_model text NOT NULL,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (version_id, embedding_model)
);

CREATE TABLE IF NOT EXISTS permissions (
    subject text NOT NULL,
    namespace text NOT NULL,
    role text NOT NULL CHECK (role IN ('reader', 'publisher', 'admin')),
    created_at timestamptz NOT NULL,
    UNIQUE (subject, namespace, role)
);

CREATE TABLE IF NOT EXISTS api_tokens (
    id uuid PRIMARY KEY,
    subject text NOT NULL,
    token_hash text NOT NULL UNIQUE,
    scopes text[] NOT NULL,
    expires_at timestamptz,
    created_at timestamptz NOT NULL,
    revoked_at timestamptz
);

CREATE TABLE IF NOT EXISTS webhook_subscriptions (
    id uuid PRIMARY KEY,
    namespace text,
    kind text NOT NULL CHECK (kind IN ('generic', 'slack', 'discord', 'matrix')),
    url text NOT NULL,
    secret_ref text,
    events text[] NOT NULL,
    enabled boolean NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id uuid PRIMARY KEY,
    subscription_id uuid NOT NULL REFERENCES webhook_subscriptions(id) ON DELETE CASCADE,
    event_type text NOT NULL,
    payload_json jsonb NOT NULL,
    status text NOT NULL CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts integer NOT NULL,
    next_attempt_at timestamptz,
    last_error text,
    created_at timestamptz NOT NULL,
    delivered_at timestamptz
);

CREATE INDEX IF NOT EXISTS idx_skill_versions_skill_version ON skill_versions(skill_id, version);
CREATE INDEX IF NOT EXISTS idx_skill_versions_yanked ON skill_versions(yanked_at);
CREATE INDEX IF NOT EXISTS idx_skills_namespace_name ON skills(namespace, name);
CREATE INDEX IF NOT EXISTS idx_skill_embeddings_vector ON skill_embeddings USING ivfflat (embedding vector_cosine_ops);

ALTER TABLE skill_versions ADD COLUMN IF NOT EXISTS artifact_digest text;
UPDATE skill_versions SET artifact_digest = digest WHERE artifact_digest IS NULL;
ALTER TABLE skill_versions ALTER COLUMN artifact_digest SET NOT NULL;
