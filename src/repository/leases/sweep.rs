//! Balayage : tâches injoignables, échéances, suspension ou saut d'horloge.

use super::{
    LeaseKey,
    contact::lease_window_seconds,
    end::end_or_log,
    outcome::{LeaseChange, LeaseEnd},
    stamp,
};
use crate::{domain::JobStatus, error::AppResult};
use chrono::{DateTime, TimeDelta, Utc};
use forgearr_shared::jobs::{LeaseProtocol, UNREACHABLE_AFTER_SECONDS};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

/// Écart d'horloge murale entre deux passages du balayage au-delà duquel le processus était
/// suspendu (ou l'horloge a sauté). Supérieur à l'attente du verrou SQLite plus l'intervalle.
pub const SWEEP_PAUSE_SECONDS: i64 = 45;

/// Passage du balayage : instant retenu et changements à notifier.
pub struct SweepPass {
    pub now: DateTime<Utc>,
    pub changes: Vec<LeaseChange>,
}

/// Balaye les baux : marque les injoignables puis traite les échéances, une transaction par tâche.
///
/// `previous` est la fin du passage précédent ; `in_flight` les baux dont le central publie le
/// résultat, jamais repris avant la fin de cette publication.
pub async fn sweep(
    pool: &SqlitePool,
    previous: Option<DateTime<Utc>>,
    in_flight: &HashSet<LeaseKey>,
) -> AppResult<SweepPass> {
    sweep_with_clock(pool, previous, in_flight, Utc::now).await
}

/// [`sweep`] avec une horloge fournie, lue une seule fois, verrou d'écriture tenu.
pub async fn sweep_with_clock(
    pool: &SqlitePool,
    previous: Option<DateTime<Utc>>,
    in_flight: &HashSet<LeaseKey>,
    clock: impl Fn() -> DateTime<Utc>,
) -> AppResult<SweepPass> {
    let timeout = crate::runtime_settings::load_unreachable_timeout(pool).await;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let now = clock();
    if let Some(shift) = previous.and_then(|previous| clock_shift(previous, now)) {
        shift_deadlines(&mut tx, now, shift, timeout).await?;
        tx.commit().await?;
        tracing::warn!(
            shift,
            "balayage des baux interrompu ou horloge déplacée : échéances décalées"
        );
        return Ok(SweepPass {
            now,
            changes: Vec::new(),
        });
    }
    let silent: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE jobs SET unreachable_since=?1 WHERE status=?2 AND unreachable_since IS NULL \
         AND (lease_contact_at IS NULL OR lease_contact_at <= datetime(?1, '-' || ?3 || ' seconds')) \
         RETURNING id",
    )
    .bind(stamp(now))
    .bind(JobStatus::Running)
    .bind(UNREACHABLE_AFTER_SECONDS)
    .fetch_all(&mut *tx)
    .await?;
    let due: Vec<LeaseKey> =
        sqlx::query_as("SELECT id, lease_epoch FROM jobs WHERE status=?1 AND leased_until <= ?2")
            .bind(JobStatus::Running)
            .bind(stamp(now))
            .fetch_all(&mut *tx)
            .await?;
    tx.commit().await?;
    let mut changes: Vec<LeaseChange> = silent.into_iter().map(LeaseChange::unreachable).collect();
    for key in due {
        if in_flight.contains(&key) {
            continue;
        }
        changes.extend(end_or_log(pool, key, LeaseEnd::Expired, Some(now)).await);
    }
    Ok(SweepPass { now, changes })
}

/// Secondes à ajouter aux échéances quand l'écart entre deux passages trahit une suspension ou un
/// saut d'horloge, dans un sens ou dans l'autre.
fn clock_shift(previous: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    let gap = (now - previous).num_seconds();
    (gap.abs() > SWEEP_PAUSE_SECONDS).then_some(gap)
}

/// Décale contacts et échéances sans les reporter au-delà de ce qu'un contact immédiat donnerait :
/// le temps déjà compté reste compté.
async fn shift_deadlines(
    conn: &mut SqliteConnection,
    now: DateTime<Utc>,
    shift: i64,
    unreachable_timeout: i64,
) -> AppResult<()> {
    let window = |protocol| {
        stamp(now + TimeDelta::seconds(lease_window_seconds(protocol, unreachable_timeout)))
    };
    sqlx::query(
        "UPDATE jobs SET \
         lease_contact_at=MIN(?1, datetime(lease_contact_at, ?2 || ' seconds')), \
         leased_until=MIN(CASE WHEN lease_protocol=?5 THEN ?3 ELSE ?4 END, \
                          datetime(leased_until, ?2 || ' seconds')) \
         WHERE status=?6 AND lease_contact_at IS NOT NULL AND leased_until IS NOT NULL",
    )
    .bind(stamp(now))
    .bind(format!("{shift:+}"))
    .bind(window(LeaseProtocol::Legacy))
    .bind(window(LeaseProtocol::Reachability))
    .bind(LeaseProtocol::Legacy.number())
    .bind(JobStatus::Running)
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_large_gap_in_either_direction_shifts_the_deadlines() {
        let previous = DateTime::<Utc>::UNIX_EPOCH;
        let after = |seconds| clock_shift(previous, previous + TimeDelta::seconds(seconds));
        assert_eq!(after(5), None);
        assert_eq!(after(SWEEP_PAUSE_SECONDS), None);
        assert_eq!(
            after(SWEEP_PAUSE_SECONDS + 1),
            Some(SWEEP_PAUSE_SECONDS + 1)
        );
        assert_eq!(after(-3600), Some(-3600));
    }
}
