//! Reprise des données enregistrées sous les identifiants d'avant le catalogue 2.0.0, pour que
//! rien n'ait à être ressaisi après la mise à jour :
//! - réglages de plugins (`plugin.sonarr` → `sonarr`) : la migration SQL
//!   `0027_plugin_ids_without_prefix.sql` renomme les lignes, puis les secrets, scellés avec
//!   l'identifiant dans leurs données associées, sont rescellés ici au démarrage ;
//! - types de blocs des flux (`block.video.encode` → `video.encode`) : définitions en base
//!   (flux et versions figées) et modèles locaux sur disque, réécrits au démarrage.

use super::{local::rebuild_local_index, secret_settings::reseal_legacy_settings};
use crate::{error::AppResult, repository::DEFINITION_STORES, secrets::SecretBox};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

/// Ancien type → type actuel des blocs officiels.
const LEGACY_BLOCK_KINDS: [(&str, &str); 24] = [
    ("block.core.file-input", "essentiel.file-input"),
    ("block.core.condition", "essentiel.condition"),
    ("block.core.webhook", "essentiel.webhook"),
    ("block.core.file-output", "essentiel.file-output"),
    ("block.media.inspect", "video.inspect"),
    ("block.metadata.mkvpropedit", "video.mkvpropedit"),
    ("block.video.remux-mkv", "video.remux-mkv"),
    ("block.video.remux-mp4", "video.remux-mp4"),
    ("block.video.encode", "video.encode"),
    ("block.audio.encode", "audio.encode"),
    ("block.sonarr.rescan", "sonarr.rescan"),
    ("block.sonarr.rename", "sonarr.rename"),
    ("block.radarr.rescan", "radarr.rescan"),
    ("block.radarr.rename", "radarr.rename"),
    ("block.scripts.run", "scripts.run"),
    ("block.emby.refresh", "emby.refresh"),
    ("block.jellyfin.refresh", "jellyfin.refresh"),
    ("block.plex.refresh", "plex.refresh"),
    ("block.notifications.apprise", "notifications.apprise"),
    ("block.notifications.discord", "notifications.discord"),
    ("block.notifications.gotify", "notifications.gotify"),
    ("block.notifications.ntfy", "notifications.ntfy"),
    ("block.subtitles.clean", "sous-titres.clean"),
    ("block.subtitles.flags", "sous-titres.flags"),
];

/// Extension des documents JSON réécrits dans le catalogue local.
const JSON_EXTENSION: &str = "json";
/// Index du catalogue local, régénéré plutôt que réécrit.
const LOCAL_INDEX_FILE: &str = "index.json";

/// Identifiant actuel → ancien identifiant des plugins officiels qui portent des réglages.
const LEGACY_PLUGIN_IDS: [(&str, &str); 11] = [
    ("essentiel", "plugin.core"),
    ("video", "plugin.video"),
    ("audio", "plugin.audio"),
    ("sous-titres", "plugin.subtitles"),
    ("scripts", "plugin.scripts"),
    ("notifications", "plugin.notifications"),
    ("sonarr", "plugin.sonarr"),
    ("radarr", "plugin.radarr"),
    ("plex", "plugin.plex"),
    ("jellyfin", "plugin.jellyfin"),
    ("emby", "plugin.emby"),
];

/// Ancien identifiant d'un plugin officiel renommé, s'il en a un.
pub fn legacy_plugin_id(plugin_id: &str) -> Option<&'static str> {
    LEGACY_PLUGIN_IDS
        .iter()
        .find(|(current, _)| *current == plugin_id)
        .map(|(_, legacy)| *legacy)
}

/// Rescelle sous leur identifiant actuel les secrets des plugins renommés, en une transaction.
/// Idempotent : renvoie le nombre de valeurs rescellées (0 une fois la reprise faite).
pub async fn migrate_legacy_plugin_secrets(
    pool: &SqlitePool,
    secrets: &SecretBox,
) -> AppResult<usize> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let settings =
        sqlx::query!(r#"SELECT plugin_id AS "plugin_id!: String", settings FROM plugin_settings"#)
            .fetch_all(&mut *tx)
            .await?;
    let mut resealed = 0;
    for row in settings {
        let Some(legacy_id) = legacy_plugin_id(&row.plugin_id) else {
            continue;
        };
        if let Some((rewritten, count)) =
            reseal_legacy_settings(secrets, &row.plugin_id, legacy_id, &row.settings)?
        {
            sqlx::query!(
                "UPDATE plugin_settings SET settings = ?1 WHERE plugin_id = ?2",
                rewritten,
                row.plugin_id
            )
            .execute(&mut *tx)
            .await?;
            resealed += count;
        }
    }
    tx.commit().await?;
    Ok(resealed)
}

/// Remplace dans un document JSON les anciens types de blocs (chaînes exactes) par les actuels ;
/// `None` si le document n'en contient aucun.
pub fn rename_legacy_block_kinds(document: &str) -> Option<String> {
    let mut renamed = document.to_owned();
    for (legacy, current) in LEGACY_BLOCK_KINDS {
        renamed = renamed.replace(&format!("\"{legacy}\""), &format!("\"{current}\""));
    }
    (renamed != document).then_some(renamed)
}

/// Réécrit les types de blocs hérités des flux en base (en une transaction) et des modèles du
/// catalogue local. Idempotent : renvoie le nombre de documents réécrits.
pub async fn migrate_legacy_block_kinds(pool: &SqlitePool, local_dir: &Path) -> AppResult<usize> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut rewritten = 0;
    for store in DEFINITION_STORES {
        for (_, rowid, definition) in store.fetch_definitions(&mut *tx).await? {
            if let Some(renamed) = rename_legacy_block_kinds(&definition) {
                store.update_definition(&mut *tx, rowid, &renamed).await?;
                rewritten += 1;
            }
        }
    }
    tx.commit().await?;

    let mut local_files = 0;
    for path in local_json_documents(local_dir) {
        let content = std::fs::read_to_string(&path)?;
        if let Some(renamed) = rename_legacy_block_kinds(&content) {
            std::fs::write(&path, renamed)?;
            local_files += 1;
        }
    }
    if local_files > 0 {
        rebuild_local_index(local_dir)?;
    }
    Ok(rewritten + local_files)
}

/// Documents JSON du catalogue local (hors index), dossiers parcourus récursivement.
fn local_json_documents(local_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![local_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some(JSON_EXTENSION)
                && path.file_name().and_then(|name| name.to_str()) != Some(LOCAL_INDEX_FILE)
            {
                found.push(path);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{decrypt_all, plugin_secret_aad};
    use serde_json::{Value, json};
    use sqlx::sqlite::SqlitePoolOptions;

    const RENAME_MIGRATION: &str =
        include_str!("../../migrations/0027_plugin_ids_without_prefix.sql");

    async fn pool() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    #[test]
    fn rename_legacy_block_kinds_rewrites_exact_kinds_only() {
        let graph = r#"{"nodes":[{"id":"n1","type":"block.video.encode"},{"id":"n2","type":"block.video.encode-x"},{"id":"n3","type":"condition"}]}"#;
        let renamed = rename_legacy_block_kinds(graph);
        assert_eq!(
            renamed.as_deref(),
            Some(
                r#"{"nodes":[{"id":"n1","type":"video.encode"},{"id":"n2","type":"block.video.encode-x"},{"id":"n3","type":"condition"}]}"#
            )
        );
        assert!(
            renamed
                .as_deref()
                .and_then(rename_legacy_block_kinds)
                .is_none()
        );
        let mut legacy: Vec<_> = LEGACY_BLOCK_KINDS.iter().map(|(old, _)| *old).collect();
        legacy.sort_unstable();
        legacy.dedup();
        assert_eq!(legacy.len(), LEGACY_BLOCK_KINDS.len());
    }

    #[tokio::test]
    async fn stored_flows_and_local_templates_get_current_block_kinds() -> anyhow::Result<()> {
        let pool = pool().await?;
        let legacy = r#"{"nodes":[{"id":"in","type":"block.core.file-input","name":"Entrée","config":{}},{"id":"enc","type":"block.video.encode","name":"Encodage","config":{}}],"connections":[{"from":"in","to":"enc"}]}"#;
        sqlx::raw_sql(&format!(
            "INSERT INTO flows (id, name, description, file_mask, is_default, definition) \
             VALUES ('flux.ancien', 'Ancien', '', '*', 0, '{legacy}'); \
             INSERT INTO flow_versions (flow_id, version, name, description, file_mask, definition) \
             VALUES ('flux.ancien', 1, 'Ancien', '', '*', '{legacy}');"
        ))
        .execute(&pool)
        .await?;
        let local = tempfile::tempdir()?;
        let template = local.path().join("templates/blocks/encode/manifest.json");
        let template_dir = template
            .parent()
            .ok_or_else(|| anyhow::anyhow!("dossier du modèle"))?;
        std::fs::create_dir_all(template_dir)?;
        std::fs::write(
            &template,
            r#"{"schemaVersion":"1.1.0","id":"template.block.encode","type":"preset","version":"1.0.0","name":"Encode","definition":{"nodes":[{"id":"n","type":"block.video.encode"}]}}"#,
        )?;

        assert_eq!(migrate_legacy_block_kinds(&pool, local.path()).await?, 3);
        assert_eq!(migrate_legacy_block_kinds(&pool, local.path()).await?, 0);

        let flow = crate::repository::get_flow(&pool, "flux.ancien").await?;
        assert!(flow.definition.contains(r#""type":"video.encode""#));
        assert!(flow.definition.contains(r#""type":"essentiel.file-input""#));
        let frozen = crate::repository::get_flow_version(&pool, "flux.ancien", 1).await?;
        assert!(!frozen.definition.contains("block."));
        assert!(std::fs::read_to_string(&template)?.contains(r#""type":"video.encode""#));
        assert!(local.path().join("index.json").is_file());
        Ok(())
    }

    #[test]
    fn every_renamed_plugin_has_a_distinct_legacy_identifier() {
        assert_eq!(legacy_plugin_id("sonarr"), Some("plugin.sonarr"));
        assert_eq!(legacy_plugin_id("essentiel"), Some("plugin.core"));
        assert_eq!(legacy_plugin_id("sous-titres"), Some("plugin.subtitles"));
        assert_eq!(legacy_plugin_id("com.alice.demo"), None);
        let mut legacy: Vec<_> = LEGACY_PLUGIN_IDS.iter().map(|(_, old)| *old).collect();
        legacy.sort_unstable();
        legacy.dedup();
        assert_eq!(legacy.len(), LEGACY_PLUGIN_IDS.len());
    }

    #[tokio::test]
    async fn legacy_settings_are_renamed_and_their_secrets_stay_readable() -> anyhow::Result<()> {
        let pool = pool().await?;
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let sealed =
            secrets.encrypt("cle-radarr", &plugin_secret_aad("plugin.radarr", "apiKey"))?;
        let changes = crate::live::ChangeNotifier::default();
        let legacy = json!({ "url": "http://radarr:7878", "apiKey": sealed }).to_string();
        crate::repository::set_plugin_settings(&pool, &changes, "plugin.radarr", &legacy).await?;
        // Réglage déjà refait sous le nouvel identifiant : il n'est jamais écrasé.
        crate::repository::set_plugin_settings(
            &pool,
            &changes,
            "sonarr",
            r#"{"url":"http://neuf"}"#,
        )
        .await?;
        crate::repository::set_plugin_settings(
            &pool,
            &changes,
            "plugin.sonarr",
            r#"{"url":"http://ancien"}"#,
        )
        .await?;

        sqlx::raw_sql(RENAME_MIGRATION).execute(&pool).await?;
        assert_eq!(migrate_legacy_plugin_secrets(&pool, &secrets).await?, 1);
        assert_eq!(migrate_legacy_plugin_secrets(&pool, &secrets).await?, 0);

        assert!(
            crate::repository::get_plugin_settings(&pool, "plugin.radarr")
                .await?
                .is_none()
        );
        let raw = crate::repository::get_plugin_settings(&pool, "radarr")
            .await?
            .ok_or_else(|| anyhow::anyhow!("réglages radarr absents"))?;
        let stored: Value = serde_json::from_str(&raw)?;
        let plain = decrypt_all(&secrets, "radarr", &stored)?;
        assert_eq!(plain["apiKey"], "cle-radarr");
        assert_eq!(plain["url"], "http://radarr:7878");

        let kept = crate::repository::get_plugin_settings(&pool, "sonarr").await?;
        assert_eq!(kept.as_deref(), Some(r#"{"url":"http://neuf"}"#));
        Ok(())
    }
}
