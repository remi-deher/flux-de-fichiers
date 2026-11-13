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
    Updates,
    Search,
    Refresh,
    Sun,
    Moon,
    Lock,
    Unlock,
    Shield,
    Check,
    Alert,
    Close,
    ChevronLeft,
    ChevronRight,
    Filters,
    Plus,
    Storage,
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
            Self::Updates => {
                r#"<path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"/><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0"/><path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"/>"#
            }
            Self::Search => {
                r#"<circle cx="11" cy="11" r="8"/><line x1="21" x2="16.65" y1="21" y2="16.65"/>"#
            }
            Self::Refresh => {
                r#"<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>"#
            }
            Self::Sun => {
                r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#
            }
            Self::Moon => r#"<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>"#,
            Self::Lock => {
                r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>"#
            }
            Self::Unlock => {
                r#"<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/>"#
            }
            Self::Shield => {
                r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#
            }
            Self::Check => r#"<polyline points="20 6 9 17 4 12"/>"#,
            Self::Alert => {
                r#"<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>"#
            }
            Self::Close => {
                r#"<line x1="18" x2="6" y1="6" y2="18"/><line x1="6" x2="18" y1="6" y2="18"/>"#
            }
            Self::ChevronLeft => r#"<path d="m15 18-6-6 6-6"/>"#,
            Self::ChevronRight => r#"<path d="m9 18 6-6-6-6"/>"#,
            Self::Filters => {
                r#"<line x1="21" x2="14" y1="4" y2="4"/><line x1="10" x2="3" y1="4" y2="4"/><line x1="21" x2="12" y1="12" y2="12"/><line x1="8" x2="3" y1="12" y2="12"/><line x1="21" x2="16" y1="20" y2="20"/><line x1="12" x2="3" y1="20" y2="20"/><line x1="14" x2="14" y1="2" y2="6"/><line x1="8" x2="8" y1="10" y2="14"/><line x1="16" x2="16" y1="18" y2="22"/>"#
            }
            Self::Plus => r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#,
            Self::Storage => {
                r#"<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14a9 3 0 0 0 18 0V5"/><path d="M3 12a9 3 0 0 0 18 0"/>"#
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_renders_svg_markup() {
        let icon_view = view! {
            <Icon name=IconName::Dashboard size=20 />
        };
        let html = icon_view.to_html();
        assert!(html.contains("<svg"));
        assert!(html.contains("width=\"20\""));
        assert!(html.contains("height=\"20\""));
        assert!(html.contains("viewBox=\"0 0 24 24\""));
        assert!(html.contains("stroke=\"currentColor\""));
        assert!(html.contains("class=\"lucide-icon\""));
    }

    #[test]
    fn test_logout_icon_has_vector_markup() {
        assert!(IconName::Logout.inner_svg().contains("<polyline"));
    }

    #[test]
    fn test_storage_icon_has_vector_markup() {
        assert!(IconName::Storage.inner_svg().contains("<ellipse"));
    }
}
