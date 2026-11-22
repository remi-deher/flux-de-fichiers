//! Événements en temps réel du central (ADR 0028) : historique borné et regroupement des changements.

mod changes;
mod history;

pub use changes::{CHANGE_WINDOW, ChangeNotifier};
pub use history::{EventHistory, LiveEvent, Subscription};
