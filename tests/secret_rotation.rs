//! Rotation de la clé de chiffrement : tout est rechiffré ou rien ne change (ADR 0011).

use flux_de_fichiers::{
    error::AppError,
    plugins::plugin_secret_aad,
    secret_rotation::{
        KeySwitch, RotationReport, reseal_and_retire, rotate_local_key, rotate_values,
        start_online_rotation, switch_active_key,
    },
    secrets::{SecretBox, retired_keys_path},
};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

const PLUGIN: &str = "sonarr";
const FLOW: &str = "flow.hook";
const NODE: &str = "n1";
const HEADER: &str = "Authorization";

fn header_aad() -> String {
    format!("flow:{FLOW}:{NODE}:{HEADER}")
}

/// Base avec un réglage de plugin secret, un en-tête de webhook secret et des valeurs en clair.
async fn seeded_pool(secrets: &SecretBox) -> anyhow::Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    seed(&pool, secrets).await?;
    Ok(pool)
}

async fn seed(pool: &sqlx::SqlitePool, secrets: &SecretBox) -> anyhow::Result<()> {
    sqlx::migrate!().run(pool).await?;
    let settings = json!({
        "url": "http://arr",
        "apiKey": secrets.encrypt("CLE-ARR", &plugin_secret_aad(PLUGIN, "apiKey"))?,
    });
    sqlx::query("INSERT INTO plugin_settings (plugin_id, settings) VALUES (?1, ?2)")
        .bind(PLUGIN)
        .bind(settings.to_string())
        .execute(pool)
        .await?;
    let definition = json!({"nodes": [
        {"id": "n0", "type": "file-input"},
        {"id": NODE, "type": "webhook", "config": {"custom_headers": {
            HEADER: secrets.encrypt("Bearer SECRET", &header_aad())?,
            "X-Clair": "valeur-publique",
        }}},
    ]});
    sqlx::query("INSERT INTO flows (id, name, definition) VALUES (?1, 'Webhook', ?2)")
        .bind(FLOW)
        .bind(definition.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

async fn stored_api_key(pool: &sqlx::SqlitePool) -> anyhow::Result<String> {
    let settings: String =
        sqlx::query_scalar("SELECT settings FROM plugin_settings WHERE plugin_id = ?1")
            .bind(PLUGIN)
            .fetch_one(pool)
            .await?;
    let value: serde_json::Value = serde_json::from_str(&settings)?;
    Ok(value["apiKey"].as_str().unwrap_or_default().to_owned())
}

async fn stored_header(pool: &sqlx::SqlitePool, header: &str) -> anyhow::Result<String> {
    let definition: String = sqlx::query_scalar("SELECT definition FROM flows WHERE id = ?1")
        .bind(FLOW)
        .fetch_one(pool)
        .await?;
    let value: serde_json::Value = serde_json::from_str(&definition)?;
    Ok(value["nodes"][1]["config"]["custom_headers"][header]
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

fn two_boxes() -> anyhow::Result<(SecretBox, SecretBox)> {
    Ok((
        SecretBox::from_key_hex(&SecretBox::generate_key_hex())?,
        SecretBox::from_key_hex(&SecretBox::generate_key_hex())?,
    ))
}

#[tokio::test]
async fn test_every_stored_secret_moves_to_the_new_key() -> anyhow::Result<()> {
    let (old, new) = two_boxes()?;
    let pool = seeded_pool(&old).await?;
    let report = rotate_values(&pool, &old, &new).await?;
    assert_eq!(
        report,
        RotationReport {
            plugin_values: 1,
            flow_values: 1
        }
    );
    let api_key = stored_api_key(&pool).await?;
    assert_eq!(
        new.decrypt(&api_key, &plugin_secret_aad(PLUGIN, "apiKey"))?,
        "CLE-ARR"
    );
    assert!(
        old.decrypt(&api_key, &plugin_secret_aad(PLUGIN, "apiKey"))
            .is_err()
    );
    let header = stored_header(&pool, HEADER).await?;
    assert_eq!(new.decrypt(&header, &header_aad())?, "Bearer SECRET");
    assert_eq!(stored_header(&pool, "X-Clair").await?, "valeur-publique");
    Ok(())
}

#[tokio::test]
async fn test_one_undecryptable_value_leaves_everything_unchanged() -> anyhow::Result<()> {
    let (old, new) = two_boxes()?;
    let pool = seeded_pool(&old).await?;
    let before = (
        stored_api_key(&pool).await?,
        stored_header(&pool, HEADER).await?,
    );
    sqlx::query("UPDATE plugin_settings SET settings = ?1 WHERE plugin_id = ?2")
        .bind(json!({"apiKey": before.0, "token": "enc:v2:cle:zz"}).to_string())
        .bind(PLUGIN)
        .execute(&pool)
        .await?;
    // Le réglage est corrompu : la rotation échoue, y compris pour l'en-tête du flux, sain.
    assert!(rotate_values(&pool, &old, &new).await.is_err());
    let header = stored_header(&pool, HEADER).await?;
    assert_eq!(header, before.1);
    assert_eq!(old.decrypt(&header, &header_aad())?, "Bearer SECRET");
    Ok(())
}

#[tokio::test]
async fn test_a_second_rotation_with_the_old_key_is_refused() -> anyhow::Result<()> {
    let (old, new) = two_boxes()?;
    let pool = seeded_pool(&old).await?;
    rotate_values(&pool, &old, &new).await?;
    assert!(rotate_values(&pool, &old, &new).await.is_err());
    Ok(())
}

const BACKGROUND_TIMEOUT: Duration = Duration::from_secs(5);
const BACKGROUND_POLL: Duration = Duration::from_millis(20);

/// Les deux secrets semés se lisent avec `secrets`.
async fn assert_readable(pool: &sqlx::SqlitePool, secrets: &SecretBox) -> anyhow::Result<()> {
    let api_key = stored_api_key(pool).await?;
    assert_eq!(
        secrets.decrypt(&api_key, &plugin_secret_aad(PLUGIN, "apiKey"))?,
        "CLE-ARR"
    );
    let header = stored_header(pool, HEADER).await?;
    assert_eq!(secrets.decrypt(&header, &header_aad())?, "Bearer SECRET");
    Ok(())
}

#[tokio::test]
async fn test_after_the_switch_old_values_stay_readable_until_resealed() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let old = SecretBox::open(dir.path())?;
    let pool = seeded_pool(&old).await?;

    let switch = switch_active_key(dir.path(), None)?;
    assert_eq!(switch.old_key_id, old.key_id());
    let ring = SecretBox::open(dir.path())?;
    assert_eq!(ring.key_id(), switch.new_key_id);
    assert_eq!(ring.retired_key_ids(), [old.key_id()]);
    assert_readable(&pool, &ring).await?;

    let report = reseal_and_retire(&pool, dir.path(), None).await?;
    assert_eq!(
        report,
        RotationReport {
            plugin_values: 1,
            flow_values: 1
        }
    );
    assert!(!retired_keys_path(dir.path()).exists());
    let new_only = SecretBox::open(dir.path())?;
    assert!(new_only.retired_key_ids().is_empty());
    assert_readable(&pool, &new_only).await?;
    // Sans clé retirée, il n'y a rien à faire.
    assert_eq!(
        reseal_and_retire(&pool, dir.path(), None).await?,
        RotationReport::default()
    );
    Ok(())
}

#[tokio::test]
async fn test_a_failed_reseal_keeps_the_retired_keys() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let old = SecretBox::open(dir.path())?;
    let pool = seeded_pool(&old).await?;
    let api_key = stored_api_key(&pool).await?;
    sqlx::query("UPDATE plugin_settings SET settings = ?1 WHERE plugin_id = ?2")
        .bind(json!({"apiKey": api_key, "token": "enc:v2:cle:zz"}).to_string())
        .bind(PLUGIN)
        .execute(&pool)
        .await?;
    switch_active_key(dir.path(), None)?;
    assert!(reseal_and_retire(&pool, dir.path(), None).await.is_err());
    let ring = SecretBox::open(dir.path())?;
    assert_eq!(ring.retired_key_ids(), [old.key_id()]);
    assert_readable(&pool, &ring).await?;
    Ok(())
}

#[tokio::test]
async fn test_an_external_key_is_never_switched() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let external = dir.path().join("cle-orchestrateur");
    std::fs::write(&external, SecretBox::generate_key_hex())?;
    assert!(matches!(
        switch_active_key(dir.path(), Some(&external)),
        Err(AppError::Conflict)
    ));
    assert!(!SecretBox::local_key_path(dir.path()).exists());
    Ok(())
}

#[tokio::test]
async fn test_local_key_rotation_replaces_the_key_and_forgets_the_old_one() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let old = SecretBox::open(dir.path())?;
    let pool = seeded_pool(&old).await?;

    let outcome = rotate_local_key(&pool, dir.path()).await?;
    assert_eq!(outcome.switch.old_key_id, old.key_id());
    assert_ne!(outcome.switch.new_key_id, outcome.switch.old_key_id);
    assert_eq!(outcome.report.plugin_values, 1);
    assert_eq!(outcome.report.flow_values, 1);

    let reopened = SecretBox::open(dir.path())?;
    assert_eq!(reopened.key_id(), outcome.switch.new_key_id);
    assert!(reopened.retired_key_ids().is_empty());
    assert_readable(&pool, &reopened).await?;
    assert!(!dir.path().join("backups").exists());
    Ok(())
}

#[tokio::test]
async fn test_online_rotation_reseals_and_retires_in_the_background() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let old = SecretBox::open(dir.path())?;
    let pool = seeded_pool(&old).await?;

    let KeySwitch { new_key_id, .. } =
        start_online_rotation(pool.clone(), dir.path().to_path_buf(), None, Duration::ZERO)?;
    let deadline = tokio::time::Instant::now() + BACKGROUND_TIMEOUT;
    while retired_keys_path(dir.path()).exists() {
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "la rotation en tâche de fond ne s'est pas achevée"
        );
        tokio::time::sleep(BACKGROUND_POLL).await;
    }
    let ring = SecretBox::open(dir.path())?;
    assert_eq!(ring.key_id(), new_key_id);
    assert!(ring.retired_key_ids().is_empty());
    assert_readable(&pool, &ring).await?;
    Ok(())
}
