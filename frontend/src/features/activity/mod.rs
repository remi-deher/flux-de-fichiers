//! Module d'activité : un registre des tâches, filtré par les onglets Exploiter, Historique et Diagnostic.

pub mod registre;

pub use registre::{ActivityRegistre, JOBS_REGISTRE_ID};

use crate::core::{SubNav, SubNavTabItem};
use crate::features::widgets::{WidgetArea, WidgetPlacement};
use leptos::prelude::*;

#[component]
pub fn JobsPanel() -> impl IntoView {
    let subnav_tabs = vec![
        SubNavTabItem::new(
            "btn-subtab-jobs-active",
            "File en direct",
            JOBS_REGISTRE_ID,
            true,
        )
        .with_icon("⚡"),
        SubNavTabItem::new(
            "btn-subtab-jobs-history",
            "Historique",
            JOBS_REGISTRE_ID,
            false,
        )
        .with_icon("📜"),
        SubNavTabItem::new(
            "btn-subtab-jobs-diagnostic",
            "Diagnostic",
            JOBS_REGISTRE_ID,
            false,
        )
        .with_icon("🔬"),
    ];

    view! {
        <div id=crate::core::panel::JOBS.dom_id class="tab-panel" role="tabpanel" aria-labelledby=crate::core::panel::JOBS.button_id hidden>
            <SubNav
                aria_label="Sous-navigation de la section Activité et Traitements"
                tabs=subnav_tabs
            />
            <p class="form-help">
                "Créer et relancer des tâches : "
                <a href=crate::APP_PATH>"nouvelle interface"</a>
            </p>
            <WidgetArea placement=WidgetPlacement::Activity />
            <ActivityRegistre />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jobs_panel_has_three_tabs_sharing_one_registre() {
        let html = view! { <JobsPanel /> }.to_html();
        for id in [
            "btn-subtab-jobs-active",
            "btn-subtab-jobs-history",
            "btn-subtab-jobs-diagnostic",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")));
        }
        assert_eq!(
            html.matches(&format!("aria-controls=\"{JOBS_REGISTRE_ID}\""))
                .count(),
            3
        );
        assert_eq!(
            html.matches(&format!("id=\"{JOBS_REGISTRE_ID}\"")).count(),
            1
        );
    }

    #[test]
    fn test_jobs_panel_links_to_the_wasm_client() {
        let html = view! { <JobsPanel /> }.to_html();
        assert!(html.contains("href=\"/app/\""));
    }

    #[test]
    fn test_jobs_panel_has_no_card_wrapper() {
        let html = view! { <JobsPanel /> }.to_html();
        assert!(!html.contains("section-panel"));
        assert!(!html.contains("toolbar-bar"));
    }
}
