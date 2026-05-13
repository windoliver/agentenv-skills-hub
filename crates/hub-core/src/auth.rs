use std::collections::{BTreeMap, BTreeSet};

use crate::{
    error::{HubError, HubResult},
    model::{NamespaceRole, Visibility},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub subject: Option<String>,
    scopes: BTreeSet<String>,
    roles: BTreeMap<String, BTreeSet<NamespaceRole>>,
}

impl AuthContext {
    pub fn anonymous() -> Self {
        Self {
            subject: None,
            scopes: BTreeSet::new(),
            roles: BTreeMap::new(),
        }
    }

    pub fn new<const S: usize, const R: usize>(
        subject: impl Into<String>,
        scopes: [&str; S],
        roles: [(&str, NamespaceRole); R],
    ) -> Self {
        let mut role_map: BTreeMap<String, BTreeSet<NamespaceRole>> = BTreeMap::new();
        for (namespace, role) in roles {
            role_map
                .entry(namespace.to_owned())
                .or_default()
                .insert(role);
        }
        Self {
            subject: Some(subject.into()),
            scopes: scopes.into_iter().map(str::to_owned).collect(),
            roles: role_map,
        }
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(scope)
    }

    pub fn has_role(&self, namespace: &str, accepted: &[NamespaceRole]) -> bool {
        self.roles
            .get(namespace)
            .is_some_and(|roles| accepted.iter().any(|role| roles.contains(role)))
    }
}

pub fn can_read(auth: &AuthContext, visibility: Visibility, namespace: &str) -> HubResult<()> {
    if visibility == Visibility::Public {
        return Ok(());
    }
    require_scope(auth, "skills:read", "read", namespace)?;
    require_role(
        auth,
        namespace,
        &[
            NamespaceRole::Reader,
            NamespaceRole::Publisher,
            NamespaceRole::Admin,
        ],
        "read",
    )
}

pub fn can_publish(auth: &AuthContext, namespace: &str) -> HubResult<()> {
    require_scope(auth, "skills:publish", "publish", namespace)?;
    require_role(
        auth,
        namespace,
        &[NamespaceRole::Publisher, NamespaceRole::Admin],
        "publish",
    )
}

pub fn can_yank(auth: &AuthContext, namespace: &str) -> HubResult<()> {
    require_scope(auth, "skills:yank", "yank", namespace)?;
    require_role(
        auth,
        namespace,
        &[NamespaceRole::Publisher, NamespaceRole::Admin],
        "yank",
    )
}

pub fn can_manage_webhooks(auth: &AuthContext, namespace: &str) -> HubResult<()> {
    require_scope(auth, "webhooks:admin", "manage_webhooks", namespace)?;
    require_role(auth, namespace, &[NamespaceRole::Admin], "manage_webhooks")
}

fn require_scope(auth: &AuthContext, scope: &str, action: &str, namespace: &str) -> HubResult<()> {
    if auth.has_scope(scope) {
        Ok(())
    } else {
        Err(HubError::PermissionDenied {
            action: action.to_owned(),
            namespace: namespace.to_owned(),
        })
    }
}

fn require_role(
    auth: &AuthContext,
    namespace: &str,
    roles: &[NamespaceRole],
    action: &str,
) -> HubResult<()> {
    if auth.has_role(namespace, roles) {
        Ok(())
    } else {
        Err(HubError::PermissionDenied {
            action: action.to_owned(),
            namespace: namespace.to_owned(),
        })
    }
}
