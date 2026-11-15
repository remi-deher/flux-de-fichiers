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
    ChevronRight,
    Logout,
    Menu,
    Command,
    Plus,
    Download,
    Sliders,
    Key,
    Package,
    Layers,
    Folder,
    Undo,
    Redo,
    Copy,
    Trash,
    Layout,
    Pin,
    Minus,
    Maximize,
    Magnet,
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
            Self::ChevronRight => r#"<path d="m9 18 6-6-6-6"/>"#,
            Self::Menu => {
                r#"<line x1="4" x2="20" y1="6" y2="6"/><line x1="4" x2="20" y1="12" y2="12"/><line x1="4" x2="20" y1="18" y2="18"/>"#
            }
            Self::Logout => {
                r#"<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" x2="9" y1="12" y2="12"/>"#
            }
            Self::Command => {
                r#"<path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z"/>"#
            }
            Self::Plus => {
                r#"<line x1="12" x2="12" y1="5" y2="19"/><line x1="5" x2="19" y1="12" y2="12"/>"#
            }
            Self::Download => {
                r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/>"#
            }
            Self::Sliders => {
                r#"<line x1="4" x2="4" y1="21" y2="14"/><line x1="4" x2="4" y1="10" y2="3"/><line x1="12" x2="12" y1="21" y2="12"/><line x1="12" x2="12" y1="8" y2="3"/><line x1="20" x2="20" y1="21" y2="16"/><line x1="20" x2="20" y1="12" y2="3"/><line x1="2" x2="6" y1="14" y2="14"/><line x1="10" x2="14" y1="8" y2="8"/><line x1="18" x2="22" y1="16" y2="16"/>"#
            }
            Self::Key => {
                r#"<circle cx="7.5" cy="15.5" r="5.5"/><path d="m21 2-9.6 9.6"/><path d="m15.5 7.5 3 3L22 7l-3-3"/>"#
            }
            Self::Package => {
                r#"<path d="m7.5 4.27 9 5.15"/><path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>"#
            }
            Self::Layers => {
                r#"<polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/>"#
            }
            Self::Folder => {
                r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#
            }
            Self::Undo => {
                r#"<path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13"/>"#
            }
            Self::Redo => {
                r#"<path d="M21 7v6h-6"/><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3l3 2.7"/>"#
            }
            Self::Copy => {
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
            }
            Self::Trash => {
                r#"<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>"#
            }
            Self::Layout => {
                r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M9 21V9"/>"#
            }
            Self::Pin => {
                r#"<line x1="12" x2="12" y1="17" y2="22"/><path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"/>"#
            }
            Self::Minus => r#"<line x1="5" x2="19" y1="12" y2="12"/>"#,
            Self::Maximize => {
                r#"<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>"#
            }
            Self::Magnet => {
                r#"<path d="m6 15-4-4 6.75-6.77a7.79 7.79 0 0 1 11 11L13 22l-4-4 6.39-6.36a2.14 2.14 0 0 0-3-3L6 15"/><path d="m5 8 4 4"/><path d="m12 15 4 4"/>"#
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

    const ALL_ICONS: [IconName; 33] = [
        IconName::Dashboard,
        IconName::Libraries,
        IconName::Flows,
        IconName::Activity,
        IconName::Workers,
        IconName::Plugins,
        IconName::Settings,
        IconName::Search,
        IconName::Refresh,
        IconName::Check,
        IconName::Alert,
        IconName::Close,
        IconName::ChevronLeft,
        IconName::ChevronRight,
        IconName::Logout,
        IconName::Menu,
        IconName::Command,
        IconName::Plus,
        IconName::Download,
        IconName::Sliders,
        IconName::Key,
        IconName::Package,
        IconName::Layers,
        IconName::Folder,
        IconName::Undo,
        IconName::Redo,
        IconName::Copy,
        IconName::Trash,
        IconName::Layout,
        IconName::Pin,
        IconName::Minus,
        IconName::Maximize,
        IconName::Magnet,
    ];

    #[test]
    fn every_icon_provides_valid_svg_markup() {
        for icon in ALL_ICONS {
            let svg = icon.inner_svg();
            assert!(!svg.is_empty());
            assert!(svg.contains('<'));
        }
    }
}
