//! Composants de l'interface, sur le gabarit et les classes du thème (`frontend/src/theme`).

pub mod drawer;
pub mod empty_state;
pub mod filters;
pub mod global_search;
pub mod icon;
pub mod metric_card;
pub mod registre;
pub mod search;
pub mod section_header;
pub mod status_badge;
pub mod subnav;

pub use drawer::Drawer;
pub use empty_state::EmptyState;
pub use icon::{Icon, IconName};
pub use metric_card::{MetricCard, MetricTone};
pub use registre::{Registre, RegistreMessage};
pub use section_header::SectionHeader;
pub use status_badge::StatusBadge;
pub use subnav::{SubNav, SubTab};
