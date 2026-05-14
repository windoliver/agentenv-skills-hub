use std::path::{Component, Path, PathBuf};

use semver::Version;
use url::Url;

use crate::error::{HubError, HubResult};

pub fn validate_namespace(value: &str) -> HubResult<()> {
    validate_ascii_identifier(value).map_err(|()| HubError::InvalidNamespace {
        value: value.to_owned(),
    })
}

pub fn validate_skill_name(value: &str) -> HubResult<()> {
    if value.is_empty()
        || value.starts_with('.')
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return Err(HubError::InvalidSkillName {
            value: value.to_owned(),
        });
    }
    Ok(())
}

pub fn validate_version(value: &str) -> HubResult<Version> {
    value
        .parse::<Version>()
        .map_err(|source| HubError::InvalidVersion {
            value: value.to_owned(),
            source,
        })
}

pub fn validate_digest(value: &str) -> HubResult<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(HubError::InvalidDigest {
            value: value.to_owned(),
        });
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(HubError::InvalidDigest {
            value: value.to_owned(),
        });
    }
    Ok(())
}

pub fn validate_skill_path(value: &str) -> HubResult<PathBuf> {
    let path = Path::new(value);
    if value.is_empty() || value.contains('\\') || has_windows_drive_prefix(value) {
        return Err(HubError::UnsafeSkillPath {
            path: path.to_path_buf(),
        });
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(HubError::UnsafeSkillPath {
                    path: path.to_path_buf(),
                });
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(HubError::UnsafeSkillPath {
            path: path.to_path_buf(),
        });
    }
    Ok(normalized)
}

pub fn validate_artifact_url(value: &str) -> HubResult<Url> {
    let url = Url::parse(value).map_err(|source| HubError::InvalidArtifactUrl {
        value: value.to_owned(),
        message: source.to_string(),
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(HubError::InvalidArtifactUrl {
            value: value.to_owned(),
            message: "artifact URL must not include user info".to_owned(),
        });
    }
    match url.scheme() {
        "oci" | "s3" | "file" => Ok(url),
        scheme => Err(HubError::InvalidArtifactUrl {
            value: value.to_owned(),
            message: format!("unsupported artifact URL scheme `{scheme}`"),
        }),
    }
}

fn validate_ascii_identifier(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.starts_with('.')
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(());
    }
    Ok(())
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
