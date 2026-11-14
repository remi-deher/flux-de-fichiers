//! Session web et identité de l'appelant : réponses de `/api/v1/auth/session` et `/api/v1/me`.

use crate::{InstanceKind, Permission, Role};
use serde::{Deserialize, Serialize};

/// Session web ouverte : le jeton CSRF accompagne toute requête qui modifie.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub csrf_token: String,
    pub expires_at: String,
    pub username: String,
}

/// Nature de l'appelant authentifié.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallerKind {
    Account,
    ApiKey,
}

/// Identité de l'appelant et ce qu'il peut faire, pour n'afficher que le permis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub kind: CallerKind,
    pub name: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub instance_kind: InstanceKind,
    pub instance_name: Option<String>,
}

impl Me {
    /// Identité renvoyée par `GET /api/v1/me`, sur un central comme sur un worker.
    pub fn new(
        kind: CallerKind,
        name: String,
        role: Role,
        instance_kind: InstanceKind,
        instance_name: Option<String>,
    ) -> Self {
        Self {
            kind,
            name,
            role,
            permissions: role.permissions(),
            instance_kind,
            instance_name,
        }
    }

    pub fn allows(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn me_keeps_the_wire_format_read_by_the_interface() -> Result<(), serde_json::Error> {
        let me = Me::new(
            CallerKind::ApiKey,
            "clé".into(),
            Role::Viewer,
            InstanceKind::Central,
            None,
        );
        let json = serde_json::to_value(&me)?;
        assert_eq!(json["kind"], "api_key");
        assert_eq!(json["role"], "viewer");
        assert_eq!(json["instanceKind"], "central");
        assert!(
            json["permissions"]
                .as_array()
                .is_some_and(|p| !p.is_empty())
        );
        assert_eq!(serde_json::from_value::<Me>(json)?, me);
        assert!(me.allows(Permission::ViewJobs));
        assert!(!me.allows(Permission::ManageJobs));
        Ok(())
    }
}
