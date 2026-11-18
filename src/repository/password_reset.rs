//! Jetons de réinitialisation du mot de passe : seul leur haché est stocké (ADR 0015).

use crate::error::AppResult;
use sqlx::SqlitePool;

/// Enregistre un jeton haché pour ce compte, valable `lifetime_seconds`.
pub async fn insert_reset_token(
    pool: &SqlitePool,
    token_hash: &str,
    account_id: &str,
    lifetime_seconds: i64,
) -> AppResult<()> {
    let modifier = format!("+{lifetime_seconds} seconds");
    sqlx::query!(
        "INSERT INTO password_reset_tokens (token_hash, account_id, expires_at) \
         VALUES (?1, ?2, datetime('now', ?3))",
        token_hash,
        account_id,
        modifier,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Consomme un jeton valide (non expiré) et renvoie son compte ; un jeton ne sert qu'une fois.
pub async fn consume_reset_token(pool: &SqlitePool, token_hash: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar!(
        "DELETE FROM password_reset_tokens WHERE token_hash = ?1 AND expires_at > datetime('now') \
         RETURNING account_id",
        token_hash,
    )
    .fetch_optional(pool)
    .await?)
}

/// Jetons émis pour ce compte depuis `seconds` secondes.
pub async fn count_recent_reset_tokens(
    pool: &SqlitePool,
    account_id: &str,
    seconds: i64,
) -> AppResult<i64> {
    let modifier = format!("-{seconds} seconds");
    Ok(sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM password_reset_tokens WHERE account_id = ?1 AND created_at > datetime('now', ?2)"#,
        account_id,
        modifier,
    )
    .fetch_one(pool)
    .await?)
}

/// Supprime tous les jetons d'un compte (nouvelle demande, mot de passe changé).
pub async fn delete_account_reset_tokens(pool: &SqlitePool, account_id: &str) -> AppResult<()> {
    sqlx::query!(
        "DELETE FROM password_reset_tokens WHERE account_id = ?1",
        account_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn purge_expired_reset_tokens(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query!("DELETE FROM password_reset_tokens WHERE expires_at <= datetime('now')")
        .execute(pool)
        .await?;
    Ok(())
}
