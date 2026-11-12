//! Gabarit partagé de sous-navigation horizontale inspiré de Watchdeck (`AppSubnav`).
//!
//! Fournit une barre d'outils collante (`page-subnav`) accueillant une capsule d'onglets
//! accessible (`app-subnav`, WAI-ARIA `tablist` / `tab`) et d'éventuelles actions spécifiques.

use leptos::prelude::*;

/// Description d'un sous-onglet individuel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubNavTabItem {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub target_id: &'static str,
    pub is_active: bool,
}

impl SubNavTabItem {
    /// Onglet sans icône ; `target_id` est l'élément qu'il pilote (`aria-controls`).
    pub const fn new(
        id: &'static str,
        label: &'static str,
        target_id: &'static str,
        is_active: bool,
    ) -> Self {
        Self {
            id,
            label,
            icon: None,
            target_id,
            is_active,
        }
    }

    pub const fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }
}

#[component]
pub fn SubNav(
    aria_label: &'static str,
    tabs: Vec<SubNavTabItem>,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <nav class="page-subnav app-subnav-toolbar" aria-label=aria_label>
            <div class="subnav-container app-subnav-container">
                <div class="app-subnav">
                    <div class="app-subnav__scroller subnav-tabs" role="tablist">
                        {tabs.into_iter().map(|item| {
                            let active_class = if item.is_active {
                                "app-subnav__item subnav-tab active"
                            } else {
                                "app-subnav__item subnav-tab"
                            };
                            let aria_selected = if item.is_active { "true" } else { "false" };
                            let tab_index = if item.is_active { "0" } else { "-1" };

                            view! {
                                <button
                                    type="button"
                                    role="tab"
                                    id=item.id
                                    class=active_class
                                    aria-selected=aria_selected
                                    aria-controls=item.target_id
                                    tabindex=tab_index
                                    data-subtab=item.target_id
                                >
                                    {item.icon.map(|ic| view! {
                                        <span class="app-subnav__icon subnav-tab-icon" aria-hidden="true">{ic}</span>
                                    })}
                                    <span class="app-subnav__label subnav-tab-label">{item.label}</span>
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                <div class="subnav-actions">
                    {actions}
                </div>
            </div>
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: &'static str, is_active: bool) -> SubNavTabItem {
        SubNavTabItem::new(id, "Onglet", "cible", is_active)
    }

    #[test]
    fn test_tab_constructor_sets_icon_only_on_request() {
        let plain = tab("a", true);
        assert_eq!(plain.icon, None);
        assert_eq!(plain.with_icon("⚡").icon, Some("⚡"));
    }

    #[test]
    fn test_subnav_renders_tabs_without_status_badges_or_info() {
        let html = view! {
            <SubNav aria_label="Test" tabs=vec![tab("a", true), tab("b", false)] />
        }
        .to_html();
        assert!(html.contains("role=\"tablist\""));
        assert!(html.contains("aria-label=\"Test\""));
        assert!(html.contains("aria-selected=\"true\""));
        assert!(html.contains("aria-selected=\"false\""));
        assert!(!html.contains("subnav-info"));
        assert!(!html.contains("badge"));
        assert!(!html.contains("status-pill"));
    }
}
