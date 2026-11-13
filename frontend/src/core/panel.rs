//! Catalogue unique des écrans : navigation Leptos et script client.

use crate::shared::ui::IconName;
use flux_shared::{InstanceKind, Permission};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panel {
    pub slug: &'static str,
    pub button_id: &'static str,
    pub dom_id: &'static str,
    pub label: &'static str,
    pub nav_title: &'static str,
    pub search_placeholder: &'static str,
    pub search_label: &'static str,
    pub icon: IconName,
    pub active: bool,
    /// Droit exigé pour voir l'écran.
    pub permission: Permission,
    /// Types d'installation qui hébergent l'écran.
    pub hosted_on: &'static [InstanceKind],
}

pub struct NavGroup {
    pub label: &'static str,
    pub panels: &'static [Panel],
}

pub const DASHBOARD: Panel = Panel {
    slug: "dashboard",
    button_id: "tab-btn-dashboard",
    dom_id: "panel-dashboard",
    label: "Vue d'ensemble",
    nav_title: "Groupe Vue d'ensemble : Statistiques et poste d'accueil",
    search_placeholder: "Rechercher dans la vue d'ensemble…",
    search_label: "Filtrer les indicateurs, statistiques et raccourcis d'accueil",
    icon: IconName::Dashboard,
    active: true,
    permission: Permission::ViewOverview,
    hosted_on: &[InstanceKind::Central, InstanceKind::Worker],
};

pub const FILES: Panel = Panel {
    slug: "files",
    button_id: "tab-btn-files",
    dom_id: "panel-files",
    label: "Bibliothèques",
    nav_title: "Groupe Médias : Bibliothèques surveillées et fichiers sources",
    search_placeholder: "Filtrer les bibliothèques…",
    search_label: "Filtrer les bibliothèques déclarées",
    icon: IconName::Libraries,
    active: false,
    permission: Permission::ViewLibraries,
    hosted_on: &[InstanceKind::Central],
};

pub const FLOWS: Panel = Panel {
    slug: "flows",
    button_id: "tab-btn-flows",
    dom_id: "panel-flows",
    label: "Flows",
    nav_title: "Groupe Traitements : Espace de conception des pipelines de traitement",
    search_placeholder: "Filtrer les flux…",
    search_label: "Rechercher parmi les flux que vous exécutez",
    icon: IconName::Flows,
    active: false,
    permission: Permission::ViewFlows,
    hosted_on: &[InstanceKind::Central],
};

pub const JOBS: Panel = Panel {
    slug: "jobs",
    button_id: "tab-btn-jobs",
    dom_id: "panel-jobs",
    label: "Activité",
    nav_title: "Groupe Traitements : Suivi en direct et analyse des exécutions",
    search_placeholder: "Filtrer les tâches et logs…",
    search_label: "Filtrer les tâches enregistrées et diagnostics",
    icon: IconName::Activity,
    active: false,
    permission: Permission::ViewJobs,
    hosted_on: &[InstanceKind::Central, InstanceKind::Worker],
};

pub const WORKERS: Panel = Panel {
    slug: "workers",
    button_id: "tab-btn-workers",
    dom_id: "panel-workers",
    label: "Workers",
    nav_title: "Groupe Infrastructure : État des workers de traitement",
    search_placeholder: "Filtrer les workers…",
    search_label: "Filtrer les serveurs et ressources distribuées",
    icon: IconName::Workers,
    active: false,
    permission: Permission::ViewWorkers,
    hosted_on: &[InstanceKind::Central],
};

pub const PLUGINS: Panel = Panel {
    slug: "plugins",
    button_id: "tab-btn-plugins",
    dom_id: "panel-plugins",
    label: "Plugins",
    nav_title: "Groupe Infrastructure : Outils média MKVToolNix, FFmpeg et extensions",
    search_placeholder: "Filtrer le catalogue…",
    search_label: "Rechercher une pièce du catalogue ou un dépôt",
    icon: IconName::Plugins,
    active: false,
    permission: Permission::ViewPlugins,
    hosted_on: &[InstanceKind::Central, InstanceKind::Worker],
};

pub const SETTINGS: Panel = Panel {
    slug: "settings",
    button_id: "tab-btn-settings",
    dom_id: "panel-settings",
    label: "Paramètres",
    nav_title: "Groupe Administration : Réglages du central, jetons et sécurité",
    search_placeholder: "Filtrer les paramètres système…",
    search_label: "Rechercher une section ou clé de configuration",
    icon: IconName::Settings,
    active: false,
    permission: Permission::ViewOverview,
    hosted_on: &[InstanceKind::Central],
};

pub const UPDATES: Panel = Panel {
    slug: "updates",
    button_id: "tab-btn-updates",
    dom_id: "panel-updates",
    label: "Mises à jour",
    nav_title: "Groupe Administration : Suivi des versions, conteneur et changelog",
    search_placeholder: "Filtrer les notes de mise à jour…",
    search_label: "Rechercher dans l'historique des versions",
    icon: IconName::Updates,
    active: false,
    permission: Permission::ManageSettings,
    hosted_on: &[InstanceKind::Central],
};

pub const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        label: "Vue générale",
        panels: &[DASHBOARD],
    },
    NavGroup {
        label: "Médias",
        panels: &[FILES],
    },
    NavGroup {
        label: "Traitements",
        panels: &[FLOWS, JOBS],
    },
    NavGroup {
        label: "Infrastructure",
        panels: &[WORKERS, PLUGINS],
    },
    NavGroup {
        label: "Système",
        panels: &[SETTINGS, UPDATES],
    },
];

pub fn panels() -> impl Iterator<Item = &'static Panel> {
    NAV_GROUPS.iter().flat_map(|group| group.panels.iter())
}

pub fn active_panel() -> &'static Panel {
    panels().find(|panel| panel.active).unwrap_or(&DASHBOARD)
}

/// Objets JavaScript `TABS`, `SEARCH_CONTEXTS` et `PANEL_ACCESS` émis depuis le catalogue.
pub fn catalog_script() -> String {
    let slugs: Vec<&str> = panels().map(|panel| panel.slug).collect();
    let contexts: serde_json::Map<String, serde_json::Value> = panels()
        .map(|panel| {
            (
                panel.slug.to_owned(),
                serde_json::json!({
                    "placeholder": panel.search_placeholder,
                    "label": panel.search_label,
                }),
            )
        })
        .collect();
    let access: serde_json::Map<String, serde_json::Value> = panels()
        .map(|panel| {
            (
                panel.slug.to_owned(),
                serde_json::json!({
                    "permission": panel.permission,
                    "hostedOn": panel.hosted_on,
                }),
            )
        })
        .collect();
    let tabs = serde_json::to_string(&slugs).unwrap_or_else(|_| "[]".to_owned());
    let contexts = serde_json::to_string(&contexts).unwrap_or_else(|_| "{}".to_owned());
    let access = serde_json::to_string(&access).unwrap_or_else(|_| "{}".to_owned());
    format!(
        "  const TABS = {tabs};\n  const SEARCH_CONTEXTS = {contexts};\n  const PANEL_ACCESS = {access};\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_one_active_panel_and_unique_slugs() {
        let listed: Vec<&Panel> = panels().collect();
        assert_eq!(listed.len(), 8);
        assert_eq!(listed.iter().filter(|panel| panel.active).count(), 1);
        assert_eq!(active_panel().slug, "dashboard");
        let mut slugs: Vec<&str> = listed.iter().map(|panel| panel.slug).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), 8);
        for panel in listed {
            assert_eq!(panel.button_id, format!("tab-btn-{}", panel.slug));
            assert_eq!(panel.dom_id, format!("panel-{}", panel.slug));
        }
    }

    #[test]
    fn catalog_script_embeds_every_screen() -> Result<(), serde_json::Error> {
        let script = catalog_script();
        let tabs: Vec<String> = serde_json::from_str(
            script
                .split("const TABS = ")
                .nth(1)
                .and_then(|rest| rest.split(';').next())
                .unwrap_or("[]"),
        )?;
        assert_eq!(tabs.len(), 8);
        for panel in panels() {
            assert!(script.contains(panel.search_placeholder));
            assert!(tabs.iter().any(|slug| slug == panel.slug));
        }
        Ok(())
    }

    #[test]
    fn every_panel_declares_a_right_and_a_hosting_instance() {
        for panel in panels() {
            assert!(!panel.hosted_on.is_empty(), "{} sans worker", panel.slug);
        }
        // Tout rôle ouvre les paramètres ; un lecteur n'y voit que son propre compte (ADR 0011).
        assert_eq!(SETTINGS.permission, Permission::ViewOverview);
        assert_eq!(UPDATES.permission, Permission::ManageSettings);
        assert_eq!(DASHBOARD.permission, Permission::ViewOverview);
        for read_only in [&DASHBOARD, &JOBS, &PLUGINS] {
            assert!(read_only.hosted_on.contains(&InstanceKind::Worker));
        }
        for central_only in [&FILES, &FLOWS, &WORKERS, &SETTINGS, &UPDATES] {
            assert_eq!(central_only.hosted_on, &[InstanceKind::Central]);
        }
    }

    #[test]
    fn catalog_script_embeds_the_access_table_with_shared_names() {
        let script = catalog_script();
        assert!(script.contains("const PANEL_ACCESS = "));
        for panel in panels() {
            assert!(script.contains(&format!("\"{}\"", panel.permission.as_str())));
        }
        assert!(script.contains("\"hostedOn\":[\"central\"]"));
        assert!(script.contains("\"hostedOn\":[\"central\",\"worker\"]"));
    }
}
