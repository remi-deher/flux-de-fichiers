//! Dépôt du plugin pour lequel des réglages ont été saisis (ADR 0029) : l'homonyme d'un autre dépôt ne
//! doit pas hériter des secrets conservés.

use crate::error::AppResult;
use forgearr_shared::plugins::store::ItemOrigin;
use sqlx::{SqliteConnection, SqlitePool};

/// Réglages enregistrés d'un plugin, secrets scellés compris : jamais journalisés.
#[derive(sqlx::FromRow)]
pub struct SavedSettings {
    pub plugin_id: String,
    pub settings: String,
    origin_repository_id: Option<String>,
    origin_repository_url: Option<String>,
}

impl SavedSettings {
    /// Peuvent-ils servir au plugin de cette origine ? Non s'ils viennent d'ailleurs ou d'une origine inconnue.
    pub fn belong_to(&self, origin: &ItemOrigin) -> bool {
        self.is_blank()
            || (self.origin_repository_id.as_deref() == Some(origin.repository_id.as_str())
                && self.origin_repository_url.as_deref() == Some(origin.repository_url.as_str()))
    }

    fn is_blank(&self) -> bool {
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&self.settings)
            .is_ok_and(|settings| settings.is_empty())
    }
}

const SELECT_SETTINGS: &str =
    "SELECT plugin_id, settings, origin_repository_id, origin_repository_url FROM plugin_settings";

pub async fn list_saved_settings(conn: &mut SqliteConnection) -> AppResult<Vec<SavedSettings>> {
    Ok(sqlx::query_as(SELECT_SETTINGS).fetch_all(conn).await?)
}

/// Les réglages conservés peuvent-ils servir au plugin de cette origine ? Oui s'il n'y en a pas.
pub async fn settings_belong_to(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    origin: &ItemOrigin,
) -> AppResult<bool> {
    let saved: Option<SavedSettings> =
        sqlx::query_as(&format!("{SELECT_SETTINGS} WHERE plugin_id = ?1"))
            .bind(plugin_id)
            .fetch_optional(conn)
            .await?;
    Ok(saved.is_none_or(|saved| saved.belong_to(origin)))
}

/// Attache les réglages d'un plugin à son origine ; sans effet s'il n'en a pas.
pub async fn set_settings_origin(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    origin: &ItemOrigin,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE plugin_settings SET origin_repository_id = ?2, origin_repository_url = ?3 WHERE plugin_id = ?1",
    )
    .bind(plugin_id)
    .bind(&origin.repository_id)
    .bind(&origin.repository_url)
    .execute(conn)
    .await?;
    Ok(())
}

/// Attache les réglages d'un plugin à l'origine de l'élément installé de ce nom ; sans effet s'il ne l'est pas.
pub async fn attach_settings_to_installed(pool: &SqlitePool, plugin_id: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE plugin_settings SET (origin_repository_id, origin_repository_url) = \
           (SELECT repository_id, repository_url FROM installed_items WHERE item_id = ?1) \
         WHERE plugin_id = ?1 AND EXISTS (SELECT 1 FROM installed_items WHERE item_id = ?1)",
    )
    .bind(plugin_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{live::ChangeNotifier, repository};
    use sqlx::sqlite::SqlitePoolOptions;

    fn origin(id: &str, url: &str) -> ItemOrigin {
        ItemOrigin {
            repository_id: id.to_owned(),
            repository_url: url.to_owned(),
            repository_name: id.to_owned(),
        }
    }

    async fn belong(pool: &SqlitePool, origin: &ItemOrigin) -> AppResult<bool> {
        let mut conn = pool.acquire().await?;
        settings_belong_to(&mut conn, "sonarr", origin).await
    }

    #[tokio::test]
    async fn kept_settings_serve_their_own_origin_only() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        let changes = ChangeNotifier::default();
        let known = origin("repo.a", "https://a.example/index.json");
        let moved = origin("repo.a", "https://b.example/index.json");
        assert!(belong(&pool, &known).await?);
        repository::set_plugin_settings(&pool, &changes, "sonarr", "{}").await?;
        assert!(belong(&pool, &known).await?);

        // Réglages saisis avant l'ADR 0029 : origine inconnue.
        let saved = r#"{"apiKey":"scellé"}"#;
        repository::set_plugin_settings(&pool, &changes, "sonarr", saved).await?;
        assert!(!belong(&pool, &known).await?);

        let mut conn = pool.acquire().await?;
        set_settings_origin(&mut conn, "sonarr", &known).await?;
        drop(conn);
        // Une nouvelle saisie garde l'origine.
        repository::set_plugin_settings(&pool, &changes, "sonarr", saved).await?;
        assert!(belong(&pool, &known).await?);
        assert!(!belong(&pool, &moved).await?);
        Ok(())
    }
}
