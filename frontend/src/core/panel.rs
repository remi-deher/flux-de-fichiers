//! Catalogue unique des écrans : navigation Leptos et script client.

use crate::shared::ui::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panel {
    pub slug: &'static str,
    pub button_id: &'static str,
    pub dom_id: &'static str,
    pub label: &'static str,
    pub nav_title: &'static str,
    pub section_title: &'static str,
    pub search_placeholder: &'static str,
    pub search_label: &'static str,
    pub icon: IconName,
    pub active: bool,
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
    section_title: "Vue d'ensemble & Statistiques",
    search_placeholder: "Rechercher dans la vue d'ensemble…",
    search_label: "Filtrer les indicateurs, statistiques et raccourcis d'accueil",
    icon: IconName::Dashboard,
    active: true,
};

pub const FILES: Panel = Panel {
    slug: "files",
    button_id: "tab-btn-files",
    dom_id: "panel-files",
    label: "Bibliothèques",
    nav_title: "Groupe Médias : Bibliothèques surveillées et fichiers sources",
    section_title: "Bibliothèques",
    search_placeholder: "Filtrer les fichiers et bibliothèques…",
    search_label: "Filtrer les fichiers médias et dossiers sources",
    icon: IconName::Libraries,
    active: false,
};

pub const FLOWS: Panel = Panel {
    slug: "flows",
    button_id: "tab-btn-flows",
    dom_id: "panel-flows",
    label: "Flows",
    nav_title: "Groupe Traitements : Espace de conception des pipelines de traitement",
    section_title: "Flows",
    search_placeholder: "Filtrer les flux…",
    search_label: "Rechercher parmi les flux que vous exécutez",
    icon: IconName::Flows,
    active: false,
};

pub const JOBS: Panel = Panel {
    slug: "jobs",
    button_id: "tab-btn-jobs",
    dom_id: "panel-jobs",
    label: "Activité",
    nav_title: "Groupe Traitements : Suivi en direct et analyse des exécutions",
    section_title: "Activité & Diagnostics",
    search_placeholder: "Filtrer les tâches et logs…",
    search_label: "Filtrer les tâches enregistrées et diagnostics",
    icon: IconName::Activity,
    active: false,
};

pub const WORKERS: Panel = Panel {
    slug: "workers",
    button_id: "tab-btn-workers",
    dom_id: "panel-workers",
    label: "Nœuds",
    nav_title: "Groupe Infrastructure : État des agents de traitement et workers",
    section_title: "Nœuds",
    search_placeholder: "Filtrer les nœuds et agents…",
    search_label: "Filtrer les serveurs et ressources distribuées",
    icon: IconName::Workers,
    active: false,
};

pub const PLUGINS: Panel = Panel {
    slug: "plugins",
    button_id: "tab-btn-plugins",
    dom_id: "panel-plugins",
    label: "Plugins",
    nav_title: "Groupe Infrastructure : Outils média MKVToolNix, FFmpeg et extensions",
    section_title: "Plugins",
    search_placeholder: "Filtrer le catalogue…",
    search_label: "Rechercher une pièce du catalogue ou un dépôt",
    icon: IconName::Plugins,
    active: false,
};

pub const SETTINGS: Panel = Panel {
    slug: "settings",
    button_id: "tab-btn-settings",
    dom_id: "panel-settings",
    label: "Paramètres",
    nav_title: "Groupe Administration : Réglages du central, jetons et sécurité",
    section_title: "Paramètres d'administration",
    search_placeholder: "Filtrer les paramètres système…",
    search_label: "Rechercher une section ou clé de configuration",
    icon: IconName::Settings,
    active: false,
};

pub const UPDATES: Panel = Panel {
    slug: "updates",
    button_id: "tab-btn-updates",
    dom_id: "panel-updates",
    label: "Mises à jour",
    nav_title: "Groupe Administration : Suivi des versions, conteneur et changelog",
    section_title: "Mises à jour & Conteneur",
    search_placeholder: "Filtrer les notes de mise à jour…",
    search_label: "Rechercher dans l'historique des versions",
    icon: IconName::Updates,
    active: false,
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

/// Objets JavaScript `TABS`, `TAB_TITLES` et `SEARCH_CONTEXTS` émis depuis le catalogue.
pub fn catalog_script() -> String {
    let slugs: Vec<&str> = panels().map(|panel| panel.slug).collect();
    let titles: serde_json::Map<String, serde_json::Value> = panels()
        .map(|panel| {
            (
                panel.slug.to_owned(),
                serde_json::Value::String(panel.section_title.to_owned()),
            )
        })
        .collect();
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
    let tabs = serde_json::to_string(&slugs).unwrap_or_else(|_| "[]".to_owned());
    let titles = serde_json::to_string(&titles).unwrap_or_else(|_| "{}".to_owned());
    let contexts = serde_json::to_string(&contexts).unwrap_or_else(|_| "{}".to_owned());
    format!(
        "  const TABS = {tabs};\n  const TAB_TITLES = {titles};\n  const SEARCH_CONTEXTS = {contexts};\n"
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
            assert!(script.contains(panel.section_title));
            assert!(script.contains(panel.search_placeholder));
            assert!(tabs.iter().any(|slug| slug == panel.slug));
        }
        Ok(())
    }
}
