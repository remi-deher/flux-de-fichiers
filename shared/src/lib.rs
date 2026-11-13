//! Types partagés entre le coordinateur et le frontend : rôles, droits, type de nœud.
//!
//! Ils sont définis une seule fois : le backend s'en sert pour contrôler l'accès de chaque route,
//! le frontend pour ne montrer que ce que l'utilisateur peut faire (ADR 0011).

use serde::{Deserialize, Serialize};

/// Type de nœud : décide de ce qui existe sur le serveur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Central,
    Worker,
}

impl NodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Central => "central",
            Self::Worker => "worker",
        }
    }
}

/// Droit unitaire sur une fonction de l'application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ViewOverview,
    ViewJobs,
    ManageJobs,
    ViewNodes,
    ManageNodes,
    ViewLibraries,
    ManageLibraries,
    ViewFlows,
    ManageFlows,
    ViewPlugins,
    ManagePlugins,
    ManageSettings,
    ManageAccounts,
    ManageApiKeys,
    /// Protocole d'agent : réclamer des tâches, transférer des fichiers, lire les contrats d'outils.
    NodeAgent,
}

impl Permission {
    pub const ALL: [Self; 15] = [
        Self::ViewOverview,
        Self::ViewJobs,
        Self::ManageJobs,
        Self::ViewNodes,
        Self::ManageNodes,
        Self::ViewLibraries,
        Self::ManageLibraries,
        Self::ViewFlows,
        Self::ManageFlows,
        Self::ViewPlugins,
        Self::ManagePlugins,
        Self::ManageSettings,
        Self::ManageAccounts,
        Self::ManageApiKeys,
        Self::NodeAgent,
    ];

    /// Nom sérialisé, identique à la valeur échangée avec le navigateur.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ViewOverview => "view_overview",
            Self::ViewJobs => "view_jobs",
            Self::ManageJobs => "manage_jobs",
            Self::ViewNodes => "view_nodes",
            Self::ManageNodes => "manage_nodes",
            Self::ViewLibraries => "view_libraries",
            Self::ManageLibraries => "manage_libraries",
            Self::ViewFlows => "view_flows",
            Self::ManageFlows => "manage_flows",
            Self::ViewPlugins => "view_plugins",
            Self::ManagePlugins => "manage_plugins",
            Self::ManageSettings => "manage_settings",
            Self::ManageAccounts => "manage_accounts",
            Self::ManageApiKeys => "manage_api_keys",
            Self::NodeAgent => "node_agent",
        }
    }
}

/// Rôle d'un compte ou d'une clé d'API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Pilote toute l'application.
    Admin,
    /// Clé d'un nœud : protocole d'agent seulement.
    Worker,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Worker => "worker",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "worker" => Some(Self::Worker),
            _ => None,
        }
    }

    /// Droits accordés par ce rôle.
    pub fn permissions(self) -> Vec<Permission> {
        Permission::ALL
            .into_iter()
            .filter(|permission| self.allows(*permission))
            .collect()
    }

    pub const fn allows(self, permission: Permission) -> bool {
        match self {
            Self::Admin => !matches!(permission, Permission::NodeAgent),
            Self::Worker => matches!(permission, Permission::NodeAgent),
        }
    }
}

/// Un panneau de l'interface est visible si le type de nœud l'héberge et que l'utilisateur a le droit.
pub fn panel_visible(
    node: NodeKind,
    hosted_on: &[NodeKind],
    required: Permission,
    granted: &[Permission],
) -> bool {
    hosted_on.contains(&node) && granted.contains(&required)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_names_match_as_str() {
        for permission in Permission::ALL {
            let serialized = serde_json::to_string(&permission).unwrap_or_default();
            assert_eq!(serialized, format!("\"{}\"", permission.as_str()));
        }
        for role in [Role::Admin, Role::Worker] {
            assert_eq!(
                serde_json::to_string(&role).unwrap_or_default(),
                format!("\"{}\"", role.as_str())
            );
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        for node in [NodeKind::Central, NodeKind::Worker] {
            assert_eq!(
                serde_json::to_string(&node).unwrap_or_default(),
                format!("\"{}\"", node.as_str())
            );
        }
        assert_eq!(Role::parse("root"), None);
    }

    #[test]
    fn test_admin_holds_every_right_except_the_agent_protocol() {
        let admin = Role::Admin.permissions();
        assert_eq!(admin.len(), Permission::ALL.len() - 1);
        assert!(!admin.contains(&Permission::NodeAgent));
        assert!(Role::Admin.allows(Permission::ManageApiKeys));
    }

    #[test]
    fn test_worker_key_is_limited_to_the_agent_protocol() {
        assert_eq!(Role::Worker.permissions(), vec![Permission::NodeAgent]);
        assert!(!Role::Worker.allows(Permission::ViewJobs));
    }

    #[test]
    fn test_panel_visibility_needs_both_the_node_kind_and_the_right() {
        let granted = Role::Admin.permissions();
        assert!(panel_visible(
            NodeKind::Central,
            &[NodeKind::Central],
            Permission::ViewLibraries,
            &granted
        ));
        assert!(!panel_visible(
            NodeKind::Worker,
            &[NodeKind::Central],
            Permission::ViewLibraries,
            &granted
        ));
        assert!(!panel_visible(
            NodeKind::Central,
            &[NodeKind::Central],
            Permission::NodeAgent,
            &granted
        ));
    }
}
