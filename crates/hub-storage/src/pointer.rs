use url::Url;

use crate::{StorageError, StorageResult};

pub fn parse_artifact_pointer(value: &str) -> StorageResult<Url> {
    let url = Url::parse(value).map_err(|source| StorageError::InvalidPointer {
        value: value.to_owned(),
        message: source.to_string(),
    })?;

    if !url.username().is_empty() || url.password().is_some() {
        return Err(StorageError::InvalidPointer {
            value: value.to_owned(),
            message: "artifact pointer must not include user info".to_owned(),
        });
    }

    match url.scheme() {
        "file" | "oci" | "s3" => Ok(url),
        scheme => Err(StorageError::InvalidPointer {
            value: value.to_owned(),
            message: format!("unsupported artifact pointer scheme `{scheme}`"),
        }),
    }
}
