//! Composants de présentation partagés purs et réutilisables.

pub mod button;
pub mod card;
pub mod detail_drawer;
pub mod empty_state;
pub mod icon;
pub mod metric_card;
pub mod section_header;
pub mod status_badge;

pub use button::{Button, ButtonVariant};
pub use card::Card;
pub use detail_drawer::DetailDrawer;
pub use empty_state::EmptyState;
pub use icon::{Icon, IconName};
pub use metric_card::MetricCard;
pub use section_header::SectionHeader;
pub use status_badge::StatusBadge;
