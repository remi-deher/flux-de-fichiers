//! Composant d'icônes vectorielles SVG modernes (Style Lucide / Linear / M3).

use leptos::prelude::*;

/// Catalogue d'icônes vectorielles standardisées pour le poste de pilotage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Dashboard,
    Libraries,
    Flows,
    Activity,
    Workers,
    Plugins,
    Settings,
    Search,
    Refresh,
    Check,
    Alert,
    Close,
    ChevronLeft,
    Logout,
}

impl IconName {
    /// Retourne les balises SVG internes de l'icône correspondante.
    pub const fn inner_svg(self) -> &'static str {
        match self {
            Self::Dashboard => {
                r#"<rect width="7" height="9" x="3" y="3" rx="1"/><rect width="7" height="5" x="14" y="3" rx="1"/><rect width="7" height="9" x="14" y="12" rx="1"/><rect width="7" height="5" x="3" y="16" rx="1"/>"#
            }
            Self::Libraries => {
                r#"<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z"/>"#
            }
            Self::Flows => r#"<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>"#,
            Self::Activity => r#"<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>"#,
            Self::Workers => {
                r#"<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/><rect width="20" height="8" x="2" y="14" rx="2" ry="2"/><line x1="6" x2="6.01" y1="6" y2="6"/><line x1="6" x2="6.01" y1="18" y2="18"/>"#
            }
            Self::Plugins => {
                r#"<path d="M12 2v4"/><path d="m19 5-3 3"/><path d="M22 12h-4"/><path d="m19 19-3-3"/><path d="M12 22v-4"/><path d="m5 19 3-3"/><path d="M2 12h4"/><path d="m5 5 3 3"/>"#
            }
            Self::Settings => {
                r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#
            }
            Self::Search => {
                r#"<circle cx="11" cy="11" r="8"/><line x1="21" x2="16.65" y1="21" y2="16.65"/>"#
            }
            Self::Refresh => {
                r#"<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>"#
            }
            Self::Check => r#"<polyline points="20 6 9 17 4 12"/>"#,
            Self::Alert => {
                r#"<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>"#
            }
            Self::Close => {
                r#"<line x1="18" x2="6" y1="6" y2="18"/><line x1="6" x2="18" y1="6" y2="18"/>"#
            }
            Self::ChevronLeft => r#"<path d="m15 18-6-6 6-6"/>"#,
            Self::Logout => {
                r#"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" x2="9" y1="12" y2="12"/>"#
            }
        }
    }
}

/// Composant d'icône vectorielle SVG moderne, réactif et accessible.
#[component]
pub fn Icon(
    name: IconName,
    #[prop(optional, default = 18)] size: u32,
    #[prop(optional)] class: Option<&'static str>,
) -> impl IntoView {
    let final_class = match class {
        Some(c) => format!("lucide-icon {c}"),
        None => "lucide-icon".to_string(),
    };

    let inner = name.inner_svg();

    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            width=size.to_string()
            height=size.to_string()
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            class=final_class
            aria-hidden="true"
            inner_html=inner
        />
    }
}
