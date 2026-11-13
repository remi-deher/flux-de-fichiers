//! Composants de présentation partagés purs et réutilisables.

pub mod button;
pub mod card;
pub mod detail_drawer;
pub mod detail_row;
pub mod empty_state;
pub mod form_field;
pub mod icon;
pub mod inspection;
pub mod metric_card;
pub mod new_interface_link;
pub mod registre;
pub mod section_header;
pub mod status_badge;
pub mod widget;

pub use button::{Button, ButtonVariant};
pub use card::Card;
pub use detail_drawer::DetailDrawer;
pub use detail_row::DetailRow;
pub use empty_state::EmptyState;
pub use form_field::{FormField, PASSWORD_MIN_LENGTH};
pub use icon::{Icon, IconName};
pub use inspection::{InspectionPane, InspectionPanel, inspection_drawer};
pub use metric_card::{MetricCard, MetricTone};
pub use new_interface_link::NewInterfaceLink;
pub use registre::RegistreTable;
pub use section_header::SectionHeader;
pub use status_badge::StatusBadge;
pub use widget::{Widget, WidgetGrid, WidgetSpan, WidgetState};
