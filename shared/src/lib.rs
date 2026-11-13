//! Types partagés entre le central et le frontend : rôles, droits, type d'installation.
//!
//! Ils sont définis une seule fois : le backend s'en sert pour contrôler l'accès de chaque route,
//! le frontend pour ne montrer que ce que l'utilisateur peut faire (ADR 0011).

use serde::{Deserialize, Serialize};

/// Type d'installation : décide de ce qui existe sur le serveur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceKind {
    Central,
    Worker,
}

impl InstanceKind {
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
    ViewWorkers,
    ManageWorkers,
    ViewLibraries,
    ManageLibraries,
    ViewFlows,
    ManageFlows,
    ViewPlugins,
    ManagePlugins,
    ManageSettings,
    ManageAccounts,
    ManageApiKeys,
    /// Protocole de worker : réclamer des tâches, transférer des fichiers, lire les contrats d'outils.
    WorkerProtocol,
}

impl Permission {
    pub const ALL: [Self; 15] = [
        Self::ViewOverview,
        Self::ViewJobs,
        Self::ManageJobs,
        Self::ViewWorkers,
        Self::ManageWorkers,
        Self::ViewLibraries,
        Self::ManageLibraries,
        Self::ViewFlows,
        Self::ManageFlows,
        Self::ViewPlugins,
        Self::ManagePlugins,
        Self::ManageSettings,
        Self::ManageAccounts,
        Self::ManageApiKeys,
        Self::WorkerProtocol,
    ];

    /// Nom sérialisé, identique à la valeur échangée avec le navigateur.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ViewOverview => "view_overview",
            Self::ViewJobs => "view_jobs",
            Self::ManageJobs => "manage_jobs",
            Self::ViewWorkers => "view_workers",
            Self::ManageWorkers => "manage_workers",
            Self::ViewLibraries => "view_libraries",
            Self::ManageLibraries => "manage_libraries",
            Self::ViewFlows => "view_flows",
            Self::ManageFlows => "manage_flows",
            Self::ViewPlugins => "view_plugins",
            Self::ManagePlugins => "manage_plugins",
            Self::ManageSettings => "manage_settings",
            Self::ManageAccounts => "manage_accounts",
            Self::ManageApiKeys => "manage_api_keys",
            Self::WorkerProtocol => "worker_protocol",
        }
    }
}

/// Rôle d'un compte ou d'une clé d'API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Pilote toute l'application.
    Admin,
    /// Clé d'un worker : protocole de worker seulement.
    Worker,
    /// Consulte l'application sans rien modifier : tous les droits « voir », aucune administration.
    Viewer,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Worker => "worker",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "worker" => Some(Self::Worker),
            "viewer" => Some(Self::Viewer),
            _ => None,
        }
    }

    /// Rôles qu'un compte humain peut porter (un worker est réservé aux clés d'API).
    pub const ACCOUNT_ROLES: [Self; 2] = [Self::Admin, Self::Viewer];

    pub fn is_account_role(self) -> bool {
        Self::ACCOUNT_ROLES.contains(&self)
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
            Self::Admin => !matches!(permission, Permission::WorkerProtocol),
            Self::Worker => matches!(permission, Permission::WorkerProtocol),
            Self::Viewer => matches!(
                permission,
                Permission::ViewOverview
                    | Permission::ViewJobs
                    | Permission::ViewWorkers
                    | Permission::ViewLibraries
                    | Permission::ViewFlows
                    | Permission::ViewPlugins
            ),
        }
    }
}

/// Un panneau de l'interface est visible si le type d'installation l'héberge et que l'utilisateur a le droit.
pub fn panel_visible(
    instance: InstanceKind,
    hosted_on: &[InstanceKind],
    required: Permission,
    granted: &[Permission],
) -> bool {
    hosted_on.contains(&instance) && granted.contains(&required)
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
        for role in [Role::Admin, Role::Worker, Role::Viewer] {
            assert_eq!(
                serde_json::to_string(&role).unwrap_or_default(),
                format!("\"{}\"", role.as_str())
            );
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        for kind in [InstanceKind::Central, InstanceKind::Worker] {
            assert_eq!(
                serde_json::to_string(&kind).unwrap_or_default(),
                format!("\"{}\"", kind.as_str())
            );
        }
        assert_eq!(Role::parse("root"), None);
    }

    #[test]
    fn test_admin_holds_every_right_except_the_agent_protocol() {
        let admin = Role::Admin.permissions();
        assert_eq!(admin.len(), Permission::ALL.len() - 1);
        assert!(!admin.contains(&Permission::WorkerProtocol));
        assert!(Role::Admin.allows(Permission::ManageApiKeys));
    }

    #[test]
    fn test_worker_key_is_limited_to_the_agent_protocol() {
        assert_eq!(Role::Worker.permissions(), vec![Permission::WorkerProtocol]);
        assert!(!Role::Worker.allows(Permission::ViewJobs));
    }

    #[test]
    fn test_viewer_can_only_look() {
        let viewer = Role::Viewer.permissions();
        assert!(!viewer.is_empty());
        for permission in Permission::ALL {
            let is_view = permission.as_str().starts_with("view_");
            assert_eq!(viewer.contains(&permission), is_view, "{permission:?}");
        }
        assert!(Role::Viewer.is_account_role() && Role::Admin.is_account_role());
        assert!(!Role::Worker.is_account_role());
    }

    #[test]
    fn test_panel_visibility_needs_both_the_instance_kind_and_the_right() {
        let granted = Role::Admin.permissions();
        assert!(panel_visible(
            InstanceKind::Central,
            &[InstanceKind::Central],
            Permission::ViewLibraries,
            &granted
        ));
        assert!(!panel_visible(
            InstanceKind::Worker,
            &[InstanceKind::Central],
            Permission::ViewLibraries,
            &granted
        ));
        assert!(!panel_visible(
            InstanceKind::Central,
            &[InstanceKind::Central],
            Permission::WorkerProtocol,
            &granted
        ));
    }
}
