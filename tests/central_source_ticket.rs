//! Les sources du central se lisent chez le worker intégré, comme chez un worker ordinaire
//! (ADR 0022) : billet de lecture, puis racine du central dans la validation du billet.

use forgearr::{error::AppError, repository};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::path::Path;
use uuid::Uuid;

const INTEGRATED_URL: &str = "http://central.lan:9000";
const INPUT_DIR: &str = "/srv/input";
const SERVING: &str = "SELECT id FROM workers WHERE serves_central=1";
const SERVING_COUNT: &str = "SELECT COUNT(*) FROM workers WHERE serves_central=1";

async fn pool() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Worker intégré (rôle de serveur des dossiers du central) et worker distant qui prend la tâche.
async fn workers(pool: &SqlitePool, announced: bool) -> anyhow::Result<(Uuid, Uuid)> {
    let integrated = repository::register(pool, "integre").await?;
    let remote = repository::register(pool, "distant").await?;
    if announced {
        repository::set_transfer_url(pool, integrated.id, Some(INTEGRATED_URL)).await?;
        repository::set_serves_central(pool, integrated.id, true).await?;
    }
    Ok((integrated.id, remote.id))
}

#[tokio::test]
async fn a_central_source_is_read_from_the_integrated_worker() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (integrated, remote) = workers(&pool, true).await?;
    let job = repository::insert_job(&pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let mut claim = repository::claim(&pool, remote)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la tâche doit être prise"))?;
    repository::attach_source_ticket(&pool, remote, &mut claim).await?;
    let ticket = claim
        .source_ticket
        .ok_or_else(|| anyhow::anyhow!("un billet est attendu"))?;
    assert_eq!(ticket.url, INTEGRATED_URL);

    // Le worker intégré valide le billet et sert le dépôt d'entrée du central.
    let input = Path::new(INPUT_DIR);
    let grant = repository::redeem_central_ticket(&pool, &ticket.token, integrated, input).await?;
    assert_eq!(grant.root, INPUT_DIR);
    assert_eq!(grant.source_name, job.source_name);
    // Un worker ordinaire ne connaît pas la racine du central.
    let ordinary = repository::redeem_ticket(&pool, &ticket.token, integrated).await;
    assert!(matches!(ordinary, Err(AppError::NotFound)));
    Ok(())
}

#[tokio::test]
async fn without_an_announced_address_a_central_source_has_no_ticket() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (_, remote) = workers(&pool, false).await?;
    repository::insert_job(&pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let mut claim = repository::claim(&pool, remote)
        .await?
        .ok_or_else(|| anyhow::anyhow!("la tâche doit être prise"))?;
    repository::attach_source_ticket(&pool, remote, &mut claim).await?;
    assert!(claim.source_ticket.is_none(), "repli par le central");
    Ok(())
}

#[tokio::test]
async fn only_one_worker_serves_the_central_folders() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (_first, second) = workers(&pool, true).await?;
    repository::set_serves_central(&pool, second, true).await?;
    let serving: Vec<Uuid> = sqlx::query_scalar(SERVING).fetch_all(&pool).await?;
    assert_eq!(serving, vec![second]);
    repository::set_serves_central(&pool, second, false).await?;
    let none: i64 = sqlx::query_scalar(SERVING_COUNT).fetch_one(&pool).await?;
    assert_eq!(none, 0);
    Ok(())
}
