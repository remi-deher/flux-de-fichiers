//! Socle central de l'application : Shell, navigation, authentification et gestion de session.

pub mod navigation;
pub mod shell;

pub use navigation::{BusinessRole, Navigation};
pub use shell::AppShell;
