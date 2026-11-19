//! Reprise des réglages de plugins enregistrés sous les identifiants d'avant le catalogue 2.0.0
//! (`plugin.sonarr` → `sonarr`), pour que les secrets n'aient jamais à être ressaisis.
//!
//! La migration SQL `0027_plugin_ids_without_prefix.sql` renomme les lignes ; les secrets, scellés
//! avec l'identifiant dans leurs données associées, sont rescellés ici au démarrage.

use super::secret_settings::reseal_legacy_settings;
use crate::{error::AppResult, secrets::SecretBox};
use sqlx::SqlitePool;

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
        let legacy = json!({ "url": "http://radarr:7878", "apiKey": sealed }).to_string();
        crate::repository::set_plugin_settings(&pool, "plugin.radarr", &legacy).await?;
        // Réglage déjà refait sous le nouvel identifiant : il n'est jamais écrasé.
        crate::repository::set_plugin_settings(&pool, "sonarr", r#"{"url":"http://neuf"}"#).await?;
        crate::repository::set_plugin_settings(
            &pool,
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
