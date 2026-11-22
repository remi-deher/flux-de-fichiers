//! Baux (ADR 0027) : contact du titulaire, état injoignable et fin de bail.
//!
//! Un bail vaut tant que son titulaire (worker, jeton, époque, `running`) est inscrit : l'échéance
//! seule ne le retire jamais, seule `end::end_lease` le fait. Toute écriture d'un titulaire passe par
//! [`ensure_held`] dans la transaction qui écrit.

mod contact;
mod end;
mod outcome;
mod restart;
mod sweep;
mod view;

pub use contact::{active, completed_by_lease, failed_by_lease, heartbeat, renew_contact};
pub(super) use contact::{ensure_held, lease_window_seconds, refresh_contact, start_lease};
pub(super) use end::set_failed;
pub use end::{LEASE_STEP, MAX_FREE_RELEASES, release_lease, undeliver_lease};
pub use outcome::{LeaseChange, LeaseEnd, LeaseOutcome};
pub use restart::{
    end_disowned_leases, end_integrated_leases, end_orphaned_lease, lease_orphaned, register_boot,
    restart_contacts,
};
pub use sweep::{SWEEP_PAUSE_SECONDS, SweepPass, sweep, sweep_with_clock};
pub use view::lease_infos;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Tâche et époque : un bail précis, jamais son successeur.
pub type LeaseKey = (Uuid, i64);

/// Horodatage au format des colonnes SQLite (`datetime('now')`).
fn stamp(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S").to_string()
}
