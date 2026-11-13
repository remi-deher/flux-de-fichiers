//! Rotation de la clé de chiffrement : tout est rechiffré ou rien ne change (ADR 0011).

use flux_de_fichiers::{
    plugins::plugin_secret_aad,
    secret_rotation::{RotationReport, rotate_local_key, rotate_values},
    secrets::SecretBox,
};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;

const PLUGIN: &str = "plugin.sonarr";
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
    sqlx::migrate!().run(&pool).await?;
    let settings = json!({
        "url": "http://arr",
        "apiKey": secrets.encrypt("CLE-ARR", &plugin_secret_aad(PLUGIN, "apiKey"))?,
    });
    sqlx::query("INSERT INTO plugin_settings (plugin_id, settings) VALUES (?1, ?2)")
        .bind(PLUGIN)
        .bind(settings.to_string())
        .execute(&pool)
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
        .execute(&pool)
        .await?;
    Ok(pool)
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

fn config_dir_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
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

#[tokio::test]
async fn test_local_key_rotation_backs_up_then_replaces_the_key_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let old = SecretBox::open(dir.path())?;
    let pool = seeded_pool(&old).await?;
    let old_key_file = std::fs::read_to_string(SecretBox::local_key_path(dir.path()))?;

    let outcome = rotate_local_key(&pool, dir.path()).await?;
    assert_eq!(outcome.old_key_id, old.key_id());
    assert_ne!(outcome.new_key_id, outcome.old_key_id);
    assert_eq!(outcome.report.plugin_values, 1);
    assert_eq!(outcome.report.flow_values, 1);

    // La clé locale a changé et déchiffre désormais les secrets.
    let reopened = SecretBox::open(dir.path())?;
    assert_eq!(reopened.key_id(), outcome.new_key_id);
    let api_key = stored_api_key(&pool).await?;
    assert_eq!(
        reopened.decrypt(&api_key, &plugin_secret_aad(PLUGIN, "apiKey"))?,
        "CLE-ARR"
    );

    // La sauvegarde contient l'ancienne clé, la nouvelle et une copie de la base.
    let files = config_dir_files(&outcome.backup_dir);
    for expected in ["secret_key", "secret_key.next", "flux.db"] {
        assert!(files.iter().any(|name| name == expected), "{expected}");
    }
    let saved_key = std::fs::read_to_string(outcome.backup_dir.join("secret_key"))?;
    assert_eq!(saved_key.trim(), old_key_file.trim());
    let saved_next = std::fs::read_to_string(outcome.backup_dir.join("secret_key.next"))?;
    assert_eq!(
        SecretBox::from_key_hex(&saved_next)?.key_id(),
        outcome.new_key_id
    );
    Ok(())
}
