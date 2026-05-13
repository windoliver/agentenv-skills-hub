use hub_core::auth::{can_manage_webhooks, can_publish, can_read, can_yank, AuthContext};
use hub_core::model::{NamespaceRole, Visibility};
use hub_core::validation::{
    validate_artifact_url, validate_digest, validate_namespace, validate_skill_name,
    validate_skill_path, validate_version,
};

#[test]
fn validates_skill_name_like_agentenv_core() {
    for valid in ["code-review", "rust_qa", "acme.skill1"] {
        validate_skill_name(valid).unwrap();
    }

    for invalid in ["", ".hidden", "Upper", "has/slash", "has space"] {
        assert!(validate_skill_name(invalid).is_err(), "{invalid} must fail");
    }
}

#[test]
fn validates_namespace_and_version() {
    validate_namespace("community").unwrap();
    validate_namespace("acme-team").unwrap();
    validate_version("1.2.3").unwrap();

    assert!(validate_namespace("").is_err());
    assert!(validate_namespace("../root").is_err());
    assert!(validate_version("latest").is_err());
}

#[test]
fn validates_digest_and_bundle_paths() {
    validate_digest("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .unwrap();
    validate_skill_path("SKILL.md").unwrap();
    validate_skill_path("references/checklist.md").unwrap();

    assert!(validate_digest("sha256:not-hex").is_err());
    assert!(validate_skill_path("../secret").is_err());
    assert!(validate_skill_path("/absolute").is_err());
    assert!(validate_skill_path("C:/secret").is_err());
    assert!(validate_skill_path("").is_err());
}

#[test]
fn validates_artifact_urls_without_user_info() {
    validate_artifact_url("oci://ghcr.io/acme/skills/code-review:1.2.3").unwrap();
    validate_artifact_url("s3://agentenv-skills/code-review/1.2.3.tar.zst").unwrap();
    validate_artifact_url("file:///tmp/code-review-1.2.3.tar.zst").unwrap();

    assert!(validate_artifact_url("https://example.com/skill.tar.zst").is_err());
    assert!(validate_artifact_url("oci://user:pass@ghcr.io/acme/skills").is_err());
}

#[test]
fn auth_allows_public_reads_without_token() {
    let auth = AuthContext::anonymous();
    assert!(can_read(&auth, Visibility::Public, "community").is_ok());
    assert!(can_read(&auth, Visibility::Private, "community").is_err());
}

#[test]
fn auth_requires_scope_and_role_for_mutations() {
    let publisher = AuthContext::new(
        "alice",
        ["skills:read", "skills:publish"],
        [("community", NamespaceRole::Publisher)],
    );
    assert!(can_publish(&publisher, "community").is_ok());
    assert!(can_yank(&publisher, "community").is_err());

    let admin = AuthContext::new(
        "bob",
        [
            "skills:read",
            "skills:publish",
            "skills:yank",
            "webhooks:admin",
        ],
        [("community", NamespaceRole::Admin)],
    );
    assert!(can_yank(&admin, "community").is_ok());
    assert!(can_manage_webhooks(&admin, "community").is_ok());
}
