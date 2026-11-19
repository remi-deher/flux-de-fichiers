//! Quotas quotidiens de l'assistant : une « question » est une conversation lancée (ADR 0015).

use crate::{
    error::{AppError, AppResult},
    repository,
};
use forgearr_shared::ai::AiQuotaSettings;
use sqlx::SqlitePool;

const QUOTAS_KEY: &str = "ai.quotas";
/// Plafond accepté pour un réglage : au-delà, autant laisser la limite ouverte (`0`).
pub const MAX_DAILY_QUOTA: u32 = forgearr_shared::ai::MAX_AI_DAILY_QUOTA;
const USAGE_RETENTION_DAYS: u32 = 14;

/// Plafonds enregistrés ; sans réglage, aucune limite.
pub async fn load_quotas(pool: &SqlitePool) -> AiQuotaSettings {
    repository::get_setting(pool, QUOTAS_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub async fn save_quotas(
    pool: &SqlitePool,
    requested: AiQuotaSettings,
) -> AppResult<AiQuotaSettings> {
    if requested.per_user_daily > MAX_DAILY_QUOTA || requested.global_daily > MAX_DAILY_QUOTA {
        return Err(AppError::Invalid);
    }
    let encoded = serde_json::to_string(&requested).map_err(|_| AppError::Internal)?;
    repository::set_setting(pool, QUOTAS_KEY, &encoded).await?;
    Ok(requested)
}

fn reached(limit: u32, used: i64) -> bool {
    limit > 0 && used >= i64::from(limit)
}

/// Compte une question de `owner`, ou `RateLimited` si l'un des plafonds du jour est atteint.
pub async fn record_question(
    pool: &SqlitePool,
    owner: &str,
    quotas: AiQuotaSettings,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let global: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(requests), 0) FROM ai_usage WHERE day = date('now')",
    )
    .fetch_one(&mut *tx)
    .await?;
    let yours: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(requests), 0) FROM ai_usage WHERE day = date('now') AND owner_id = ?1",
    )
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    if reached(quotas.global_daily, global) || reached(quotas.per_user_daily, yours) {
        return Err(AppError::RateLimited);
    }
    sqlx::query(
        "INSERT INTO ai_usage (day, owner_id, requests) VALUES (date('now'), ?1, 1)
         ON CONFLICT (day, owner_id) DO UPDATE SET requests = requests + 1",
    )
    .bind(owner)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM ai_usage WHERE day < date('now', ?1)")
        .bind(format!("-{USAGE_RETENTION_DAYS} days"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Questions du jour : `(toute l'installation, ce compte)`.
pub async fn requests_today(pool: &SqlitePool, owner: &str) -> AppResult<(u32, u32)> {
    let global: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(requests), 0) FROM ai_usage WHERE day = date('now')",
    )
    .fetch_one(pool)
    .await?;
    let yours: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(requests), 0) FROM ai_usage WHERE day = date('now') AND owner_id = ?1",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?;
    Ok((
        u32::try_from(global).unwrap_or(u32::MAX),
        u32::try_from(yours).unwrap_or(u32::MAX),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::settings::tests_support::test_pool;

    #[tokio::test]
    async fn quotas_default_to_open_and_reject_absurd_values() -> anyhow::Result<()> {
        let pool = test_pool().await?;
        assert_eq!(load_quotas(&pool).await, AiQuotaSettings::default());
        let saved = save_quotas(
            &pool,
            AiQuotaSettings {
                per_user_daily: 3,
                global_daily: 5,
            },
        )
        .await?;
        assert_eq!(load_quotas(&pool).await, saved);
        let absurd = AiQuotaSettings {
            per_user_daily: MAX_DAILY_QUOTA + 1,
            global_daily: 0,
        };
        assert!(matches!(
            save_quotas(&pool, absurd).await,
            Err(AppError::Invalid)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn per_user_and_global_limits_stop_questions_and_zero_means_open() -> anyhow::Result<()> {
        let pool = test_pool().await?;
        let quotas = AiQuotaSettings {
            per_user_daily: 2,
            global_daily: 3,
        };
        record_question(&pool, "alice", quotas).await?;
        record_question(&pool, "alice", quotas).await?;
        assert!(matches!(
            record_question(&pool, "alice", quotas).await,
            Err(AppError::RateLimited)
        ));
        // Une question refusée n'est pas comptée ; bob a sa propre part.
        record_question(&pool, "bob", quotas).await?;
        // Le plafond global (3) est atteint : plus personne ne passe.
        assert!(matches!(
            record_question(&pool, "bob", quotas).await,
            Err(AppError::RateLimited)
        ));
        assert_eq!(requests_today(&pool, "alice").await?, (3, 2));
        let open = AiQuotaSettings::default();
        for _ in 0..10 {
            record_question(&pool, "carol", open).await?;
        }
        assert_eq!(requests_today(&pool, "carol").await?, (13, 10));
        Ok(())
    }

    #[tokio::test]
    async fn old_days_do_not_count_and_are_purged() -> anyhow::Result<()> {
        let pool = test_pool().await?;
        sqlx::query("INSERT INTO ai_usage (day, owner_id, requests) VALUES (date('now', '-1 day'), 'alice', 99)")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO ai_usage (day, owner_id, requests) VALUES (date('now', '-20 days'), 'alice', 5)")
            .execute(&pool)
            .await?;
        let quotas = AiQuotaSettings {
            per_user_daily: 1,
            global_daily: 1,
        };
        record_question(&pool, "alice", quotas).await?;
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_usage")
            .fetch_one(&pool)
            .await?;
        assert_eq!(
            rows, 2,
            "la ligne de 20 jours est purgée, celle d'hier reste"
        );
        Ok(())
    }
}
