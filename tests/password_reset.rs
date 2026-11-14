//! Réinitialisation d'un mot de passe oublié en ligne de commande (ADR 0011).

use flux_de_fichiers::{account_admin::reset_password, error::AppError, password, repository};
use flux_shared::Role;
use sqlx::sqlite::SqlitePoolOptions;

const USERNAME: &str = "admin";
const OLD_PASSWORD: &str = "ancien-mot-de-passe";

async fn pool_with_admin() -> anyhow::Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let hash = password::hash(OLD_PASSWORD.into())
        .await
        .map_err(|_| anyhow::anyhow!("hachage"))?;
    repository::create_first_account(&pool, USERNAME, &hash, Role::Admin, None)
        .await
        .map_err(|_| anyhow::anyhow!("création du compte"))?;
    Ok(pool)
}

async fn stored_hash(pool: &sqlx::SqlitePool) -> anyhow::Result<String> {
    let (_, hash) = repository::find_account_by_username(pool, USERNAME)
        .await
        .map_err(|_| anyhow::anyhow!("lecture"))?
        .ok_or_else(|| anyhow::anyhow!("compte absent"))?;
    Ok(hash)
}

#[tokio::test]
async fn test_reset_replaces_the_password_and_returns_it_once() -> anyhow::Result<()> {
    let pool = pool_with_admin().await?;
    let new_password = reset_password(&pool, "  Admin ")
        .await
        .map_err(|_| anyhow::anyhow!("réinitialisation"))?;
    let hash = stored_hash(&pool).await?;
    assert!(password::verify(new_password.clone(), hash.clone()).await);
    assert!(!password::verify(OLD_PASSWORD.into(), hash).await);
    assert_ne!(new_password, OLD_PASSWORD);
    Ok(())
}

#[tokio::test]
async fn test_reset_of_an_unknown_account_is_not_found() -> anyhow::Result<()> {
    let pool = pool_with_admin().await?;
    let result = reset_password(&pool, "inconnu").await;
    assert!(matches!(result, Err(AppError::NotFound)));
    assert!(password::verify(OLD_PASSWORD.into(), stored_hash(&pool).await?).await);
    Ok(())
}

#[tokio::test]
async fn test_reset_closes_every_session_of_the_account() -> anyhow::Result<()> {
    let pool = pool_with_admin().await?;
    let (account, _) = repository::find_account_by_username(&pool, USERNAME)
        .await
        .map_err(|_| anyhow::anyhow!("lecture"))?
        .ok_or_else(|| anyhow::anyhow!("compte absent"))?;
    let session = flux_de_fichiers::web_session::create(&pool, 600, &account.id)
        .await
        .map_err(|_| anyhow::anyhow!("session"))?;
    let token_hash = flux_de_fichiers::web_session::digest(&session.token);
    let open = |pool: sqlx::SqlitePool, hash: String| async move {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE token_hash = ?1")
            .bind(hash)
            .fetch_one(&pool)
            .await
    };
    assert_eq!(open(pool.clone(), token_hash.clone()).await?, 1);
    reset_password(&pool, USERNAME)
        .await
        .map_err(|_| anyhow::anyhow!("réinitialisation"))?;
    assert_eq!(open(pool, token_hash).await?, 0);
    Ok(())
}
