//! Composants de l'interface, sur le gabarit et les classes du thème (`frontend/src/theme`).

pub mod command_palette;
pub mod drawer;
pub mod empty_state;
pub mod filters;
pub mod global_search;
pub mod icon;
pub mod metric_card;
pub mod page_header;
pub mod progress_bar;
pub mod registre;
pub mod search;
pub mod section_header;
pub mod stat_strip;
pub mod status_badge;
pub mod toast;
pub mod topbar_action;

pub use command_palette::{CommandPalette, CommandPaletteState};
pub use drawer::Drawer;
pub use empty_state::EmptyState;
pub use icon::{Icon, IconName};
pub use metric_card::{MetricCard, MetricTone};
pub use page_header::breadcrumb_for;
pub use progress_bar::ProgressBar;
pub use registre::{Registre, RegistreMessage};
pub use section_header::SectionHeader;
pub use stat_strip::{StatItem, StatStrip};
pub use status_badge::StatusBadge;
pub use toast::{ToastViewport, Toasts};
