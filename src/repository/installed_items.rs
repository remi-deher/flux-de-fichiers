//! Éléments installés (ADR 0029). Écritures dans la transaction de l'appelant, qui vide ensuite le
//! cache du catalogue : c'est elle qui signale le changement (ADR 0028).
//!
//! Requêtes sous forme runtime : le cache SQLx hors ligne ne connaît pas cette table.

use crate::error::{AppError, AppResult};
use forgearr_shared::plugins::{store::ItemOrigin, vocab::CatalogItemType};
use sqlx::{SqliteConnection, SqlitePool};

/// Ce qu'une installation inscrit. La copie figée se déduit de `item_id` et `content_sha256`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationRecord {
    pub item_id: String,
    pub kind: CatalogItemType,
    pub version: String,
    pub content_sha256: String,
    pub origin: ItemOrigin,
    /// Flux : dernier flux créé ; script : identifiant dans la collection locale.
    pub copy_id: Option<String>,
    /// La copie modifiable reste à écrire depuis la copie figée.
    pub copy_pending: bool,
    /// L'élément porte une sonde ou une opération par défaut : toute tâche peut s'en servir.
    pub serves_every_task: bool,
}

/// Élément inscrit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installation {
    pub record: InstallationRecord,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(sqlx::FromRow)]
struct InstallationRow {
    item_id: String,
    kind: String,
    version: String,
    content_sha256: String,
    repository_id: String,
    repository_url: String,
    repository_name: String,
    copy_id: Option<String>,
    copy_pending: bool,
    serves_every_task: bool,
    installed_at: String,
    updated_at: String,
}

impl TryFrom<InstallationRow> for Installation {
    type Error = AppError;

    fn try_from(row: InstallationRow) -> AppResult<Self> {
        let Some(kind) = CatalogItemType::parse(&row.kind) else {
            tracing::error!(item = %row.item_id, kind = %row.kind, "type d'élément installé inconnu");
            return Err(AppError::Internal);
        };
        Ok(Self {
            record: InstallationRecord {
                item_id: row.item_id,
                kind,
                version: row.version,
                content_sha256: row.content_sha256,
                origin: ItemOrigin {
                    repository_id: row.repository_id,
                    repository_url: row.repository_url,
                    repository_name: row.repository_name,
                },
                copy_id: row.copy_id,
                copy_pending: row.copy_pending,
                serves_every_task: row.serves_every_task,
            },
            installed_at: row.installed_at,
            updated_at: row.updated_at,
        })
    }
}

const SELECT_INSTALLATIONS: &str = "SELECT item_id, kind, version, content_sha256, repository_id, repository_url, repository_name, copy_id, copy_pending, serves_every_task, installed_at, updated_at FROM installed_items";

/// Éléments installés, du plus ancien au plus récent : ordre stable, qu'une mise à jour ne change pas. À la
/// même seconde, celui de leur inscription départage (le dernier d'un conflit d'opération gagne).
pub async fn list_installations(pool: &SqlitePool) -> AppResult<Vec<Installation>> {
    sqlx::query_as::<_, InstallationRow>(&format!(
        "{SELECT_INSTALLATIONS} ORDER BY installed_at, rowid"
    ))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(Installation::try_from)
    .collect()
}

pub async fn get_installation(
    conn: &mut SqliteConnection,
    item_id: &str,
) -> AppResult<Option<Installation>> {
    sqlx::query_as::<_, InstallationRow>(&format!("{SELECT_INSTALLATIONS} WHERE item_id = ?1"))
        .bind(item_id)
        .fetch_optional(conn)
        .await?
        .map(Installation::try_from)
        .transpose()
}

/// Inscrit un élément, ou le met à jour en gardant sa date d'installation.
pub async fn save_installation(
    conn: &mut SqliteConnection,
    record: &InstallationRecord,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO installed_items (item_id, kind, version, content_sha256, repository_id, repository_url, repository_name, copy_id, copy_pending, serves_every_task) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(item_id) DO UPDATE SET kind = ?2, version = ?3, content_sha256 = ?4, \
         repository_id = ?5, repository_url = ?6, repository_name = ?7, copy_id = ?8, \
         copy_pending = ?9, serves_every_task = ?10, updated_at = datetime('now')",
    )
    .bind(&record.item_id)
    .bind(record.kind.as_str())
    .bind(&record.version)
    .bind(&record.content_sha256)
    .bind(&record.origin.repository_id)
    .bind(&record.origin.repository_url)
    .bind(&record.origin.repository_name)
    .bind(&record.copy_id)
    .bind(record.copy_pending)
    .bind(record.serves_every_task)
    .execute(conn)
    .await?;
    Ok(())
}

/// La copie modifiable de cette empreinte est écrite. Sans effet si l'élément a changé entre-temps.
pub async fn finish_installation_copy(
    conn: &mut SqliteConnection,
    item_id: &str,
    content_sha256: &str,
) -> AppResult<bool> {
    let done = sqlx::query(
        "UPDATE installed_items SET copy_pending = 0 WHERE item_id = ?1 AND content_sha256 = ?2",
    )
    .bind(item_id)
    .bind(content_sha256)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Retire un élément ; `false` s'il n'était pas installé.
pub async fn delete_installation(conn: &mut SqliteConnection, item_id: &str) -> AppResult<bool> {
    let deleted = sqlx::query("DELETE FROM installed_items WHERE item_id = ?1")
        .bind(item_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Retire l'élément dont la copie modifiable (flux ou script) vient d'être supprimée.
pub async fn delete_installation_of_copy(
    conn: &mut SqliteConnection,
    kind: CatalogItemType,
    copy_id: &str,
) -> AppResult<bool> {
    let deleted = sqlx::query("DELETE FROM installed_items WHERE kind = ?1 AND copy_id = ?2")
        .bind(kind.as_str())
        .bind(copy_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{opaque_token::sha256_hex, repository};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    fn record(item_id: &str, kind: CatalogItemType, copy_id: Option<&str>) -> InstallationRecord {
        InstallationRecord {
            item_id: item_id.to_owned(),
            kind,
            version: "1.0.0".to_owned(),
            content_sha256: sha256_hex(item_id),
            origin: ItemOrigin {
                repository_id: "repo.test".to_owned(),
                repository_url: "https://example.org/index.json".to_owned(),
                repository_name: "Test".to_owned(),
            },
            copy_id: copy_id.map(str::to_owned),
            copy_pending: copy_id.is_some(),
            serves_every_task: item_id == "video",
        }
    }

    #[tokio::test]
    async fn an_update_keeps_one_line_and_its_installation_date() -> anyhow::Result<()> {
        let pool = pool().await?;
        let mut conn = pool.acquire().await?;
        let first = record("video", CatalogItemType::Plugin, None);
        save_installation(&mut conn, &first).await?;
        sqlx::query("UPDATE installed_items SET installed_at = '2020-01-01 00:00:00'")
            .execute(&mut *conn)
            .await?;
        let update = InstallationRecord {
            version: "1.1.0".to_owned(),
            content_sha256: sha256_hex("video 1.1.0"),
            ..first
        };
        save_installation(&mut conn, &update).await?;
        save_installation(&mut conn, &record("audio", CatalogItemType::Plugin, None)).await?;
        drop(conn);

        let listed = list_installations(&pool).await?;
        let ids: Vec<&str> = listed
            .iter()
            .map(|item| item.record.item_id.as_str())
            .collect();
        assert_eq!(ids, ["video", "audio"]);
        assert_eq!(listed[0].record, update);
        assert_eq!(listed[0].installed_at, "2020-01-01 00:00:00");
        Ok(())
    }

    #[tokio::test]
    async fn the_origin_outlives_its_repository() -> anyhow::Result<()> {
        let pool = pool().await?;
        let changes = crate::live::ChangeNotifier::default();
        let item = record("video", CatalogItemType::Plugin, None);
        let origin = &item.origin;
        repository::add_plugin_repository(
            &pool,
            &changes,
            &origin.repository_id,
            &origin.repository_name,
            &origin.repository_url,
        )
        .await?;
        let mut conn = pool.acquire().await?;
        save_installation(&mut conn, &item).await?;
        drop(conn);
        repository::delete_plugin_repository(&pool, &changes, &origin.repository_id).await?;
        let mut conn = pool.acquire().await?;
        let kept = get_installation(&mut conn, "video").await?;
        assert_eq!(kept.map(|kept| kept.record), Some(item));
        Ok(())
    }

    #[tokio::test]
    async fn a_copy_is_held_by_one_item_and_frees_it_when_deleted() -> anyhow::Result<()> {
        let pool = pool().await?;
        let mut conn = pool.acquire().await?;
        let script = record("demo", CatalogItemType::Script, Some("demo"));
        save_installation(&mut conn, &script).await?;
        let twin = record("autre", CatalogItemType::Script, Some("demo"));
        assert!(matches!(
            save_installation(&mut conn, &twin).await,
            Err(AppError::Conflict)
        ));

        // Une copie finie pour un contenu déjà remplacé ne vaut pas pour le suivant.
        assert!(!finish_installation_copy(&mut conn, "demo", &sha256_hex("autre")).await?);
        assert!(finish_installation_copy(&mut conn, "demo", &script.content_sha256).await?);
        let finished = get_installation(&mut conn, "demo").await?;
        assert!(finished.is_some_and(|item| !item.record.copy_pending));

        let flow = CatalogItemType::Flow;
        assert!(!delete_installation_of_copy(&mut conn, flow, "demo").await?);
        assert!(delete_installation_of_copy(&mut conn, CatalogItemType::Script, "demo").await?);
        assert!(!delete_installation(&mut conn, "demo").await?);
        Ok(())
    }

    #[tokio::test]
    async fn packs_and_blocks_are_never_recorded() -> anyhow::Result<()> {
        let pool = pool().await?;
        let mut conn = pool.acquire().await?;
        for kind in [CatalogItemType::Pack, CatalogItemType::Block] {
            let refused = save_installation(&mut conn, &record("x", kind, None)).await;
            assert!(matches!(refused, Err(AppError::Invalid)), "{kind:?}");
        }
        Ok(())
    }
}
