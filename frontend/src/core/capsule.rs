//! Actions de la capsule de recherche : un registre unique, rendu par un composant commun.
//!
//! La capsule porte les actions du contexte courant, puis le bouton de filtres (rendu par
//! l'en-tête). Une action n'est visible que dans le contexte qui la déclare ; le script
//! client bascule l'attribut `hidden` d'après `data-capsule-tab`. Une zone d'outils
//! secondaires pourra s'ajouter quand une page en aura besoin.

use super::panel::{FILES, FLOWS, PLUGINS, Panel, SETTINGS, WORKERS};
use crate::shared::ui::{Icon, IconName};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Taille de l'icône d'une action de capsule, en pixels.
const CAPSULE_ICON_SIZE: u32 = 14;

/// Contexte d'affichage d'une action : une page, ou un état particulier de cette page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapsuleContext {
    Files,
    Flows,
    FlowEditor,
    Workers,
    Plugins,
    Settings,
}

impl CapsuleContext {
    /// Valeur de `data-capsule-tab`, partagée avec le script client.
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Flows => "flows",
            Self::FlowEditor => "flow-editor",
            Self::Workers => "workers",
            Self::Plugins => "plugins",
            Self::Settings => "settings",
        }
    }

    /// Page du catalogue qui porte ce contexte.
    pub const fn panel(self) -> Panel {
        match self {
            Self::Files => FILES,
            Self::Flows | Self::FlowEditor => FLOWS,
            Self::Workers => WORKERS,
            Self::Plugins => PLUGINS,
            Self::Settings => SETTINGS,
        }
    }
}

/// Poids visuel d'une action : l'action principale de la page, ou une action secondaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapsuleTone {
    Primary,
    Secondary,
}

impl CapsuleTone {
    pub const fn as_class(self) -> &'static str {
        match self {
            Self::Primary => "search-capsule__action search-capsule__action--primary",
            Self::Secondary => "search-capsule__action",
        }
    }
}

/// Une action de capsule, visible dans un seul contexte.
pub struct CapsuleAction {
    pub id: &'static str,
    pub label: &'static str,
    pub aria_label: Option<&'static str>,
    pub icon: IconName,
    pub tone: CapsuleTone,
    pub context: CapsuleContext,
}

pub const CAPSULE_ACTIONS: &[CapsuleAction] = &[
    CapsuleAction {
        id: "library-create-btn",
        label: "Déclarer",
        aria_label: Some("Déclarer une bibliothèque"),
        icon: IconName::Plus,
        tone: CapsuleTone::Primary,
        context: CapsuleContext::Files,
    },
    CapsuleAction {
        id: "btn-open-flow-wizard",
        label: "Nouveau flux",
        aria_label: None,
        icon: IconName::Plus,
        tone: CapsuleTone::Primary,
        context: CapsuleContext::Flows,
    },
    CapsuleAction {
        id: "btn-back-to-library",
        label: "Retour aux flux",
        aria_label: Some("Retourner à la liste des flux"),
        icon: IconName::ChevronLeft,
        tone: CapsuleTone::Secondary,
        context: CapsuleContext::FlowEditor,
    },
    CapsuleAction {
        id: "btn-canvas-save-flow",
        label: "Enregistrer",
        aria_label: Some("Enregistrer les modifications du flux"),
        icon: IconName::Check,
        tone: CapsuleTone::Primary,
        context: CapsuleContext::FlowEditor,
    },
    CapsuleAction {
        id: "btn-workers-connect",
        label: "Raccorder un worker",
        aria_label: Some("Raccorder un nouveau worker"),
        icon: IconName::Plus,
        tone: CapsuleTone::Primary,
        context: CapsuleContext::Workers,
    },
    CapsuleAction {
        id: "api-key-new-btn",
        label: "Créer une clé",
        aria_label: Some("Créer une clé d'API"),
        icon: IconName::Plus,
        tone: CapsuleTone::Primary,
        context: CapsuleContext::Settings,
    },
    CapsuleAction {
        id: "btn-open-add-repo-dialog",
        label: "Ajouter un dépôt",
        aria_label: Some("Ajouter un nouveau dépôt de plugins"),
        icon: IconName::Plus,
        tone: CapsuleTone::Primary,
        context: CapsuleContext::Plugins,
    },
];

/// Boutons d'action de la capsule, masqués tant que leur contexte n'est pas actif.
#[component]
pub fn CapsuleActions() -> impl IntoView {
    view! {
        <div class="search-capsule__actions">
            {CAPSULE_ACTIONS
                .iter()
                .map(|action| view! {
                    <button
                        type="button"
                        id=action.id
                        class=action.tone.as_class()
                        data-capsule-tab=action.context.as_attr()
                        aria-label=action.aria_label
                        hidden
                    >
                        <Icon name=action.icon size=CAPSULE_ICON_SIZE />
                        {action.label}
                    </button>
                })
                .collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::panel::panels;
    use std::collections::HashSet;

    const ALL_CONTEXTS: [CapsuleContext; 6] = [
        CapsuleContext::Files,
        CapsuleContext::Flows,
        CapsuleContext::FlowEditor,
        CapsuleContext::Workers,
        CapsuleContext::Plugins,
        CapsuleContext::Settings,
    ];

    #[test]
    fn test_context_attributes_match_serde_names() {
        for context in ALL_CONTEXTS {
            let serialized = serde_json::to_string(&context).unwrap_or_default();
            assert_eq!(serialized, format!("\"{}\"", context.as_attr()));
        }
    }

    #[test]
    fn test_every_context_belongs_to_a_known_panel() {
        for context in ALL_CONTEXTS {
            assert!(panels().any(|panel| panel.slug == context.panel().slug));
        }
    }

    #[test]
    fn test_page_contexts_use_the_panel_slug() {
        assert_eq!(CapsuleContext::Files.as_attr(), FILES.slug);
        assert_eq!(CapsuleContext::Flows.as_attr(), FLOWS.slug);
        assert_eq!(CapsuleContext::Workers.as_attr(), WORKERS.slug);
        assert_eq!(CapsuleContext::Plugins.as_attr(), PLUGINS.slug);
        assert_eq!(CapsuleContext::Settings.as_attr(), SETTINGS.slug);
        assert_eq!(CapsuleContext::FlowEditor.panel().slug, FLOWS.slug);
    }

    #[test]
    fn test_tone_classes_match_serde_names() {
        assert!(CapsuleTone::Primary.as_class().contains("--primary"));
        assert!(!CapsuleTone::Secondary.as_class().contains("--primary"));
        assert_eq!(
            serde_json::to_string(&CapsuleTone::Secondary).unwrap_or_default(),
            "\"secondary\""
        );
    }

    #[test]
    fn test_action_ids_are_unique() {
        let ids: HashSet<_> = CAPSULE_ACTIONS.iter().map(|action| action.id).collect();
        assert_eq!(ids.len(), CAPSULE_ACTIONS.len());
    }

    #[test]
    fn test_actions_render_hidden_buttons_bound_to_their_context() {
        let html = view! { <CapsuleActions /> }.to_html();
        assert_eq!(html.matches(" hidden").count(), CAPSULE_ACTIONS.len());
        for action in CAPSULE_ACTIONS {
            assert!(html.contains(&format!("id=\"{}\"", action.id)));
            assert!(html.contains(&format!(
                "data-capsule-tab=\"{}\"",
                action.context.as_attr()
            )));
        }
    }

    #[test]
    fn test_flow_editor_context_has_back_and_save_actions() {
        let editor_ids: Vec<_> = CAPSULE_ACTIONS
            .iter()
            .filter(|action| action.context == CapsuleContext::FlowEditor)
            .map(|action| action.id)
            .collect();
        assert_eq!(editor_ids, ["btn-back-to-library", "btn-canvas-save-flow"]);
    }
}
