//! Socle central de l'application : Shell, navigation, authentification et gestion de session.

pub mod navigation;
pub mod panel;
pub mod shell;
pub mod subnav;

pub use navigation::{BusinessGroup, BusinessRole, Navigation};
pub use panel::{Panel, active_panel, panels};
pub use shell::{AppShell, Header, Sidebar};
pub use subnav::{SubNav, SubNavTabItem};
