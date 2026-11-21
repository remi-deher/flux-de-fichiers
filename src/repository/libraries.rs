//! Persistance des bibliothèques déclarées et de leurs chemins par worker.

use crate::{
    domain::{
        JOB_PRIORITY_MAX, JOB_PRIORITY_MIN, LIBRARY_NAME_MAX, LIBRARY_PATH_MAX, Library,
        LibraryAccess, LibraryOutputPolicy, LibraryWorkerPath, LibraryWrite,
        MAX_SCAN_INTERVAL_MINUTES,
    },
    error::{AppError, AppResult},
};
use forgearr_shared::jobs::LIBRARY_DEFAULT_JOB_PRIORITY;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

struct LibraryRow {
    id: Uuid,
    name: String,
    flow_id: Option<String>,
    scan_interval_minutes: i32,
    last_scanned_at: Option<chrono::DateTime<chrono::Utc>>,
    include_extensions: String,
    exclude_patterns: String,
    min_file_bytes: i64,
    output_policy: LibraryOutputPolicy,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct LibraryWorkerRow {
    library_id: Uuid,
    worker_id: Option<Uuid>,
    source_key: Option<String>,
    local_path: String,
    access: LibraryAccess,
}

struct PreparedLibrary {
    name: String,
    flow_id: Option<String>,
    workers: Vec<LibraryWorkerPath>,
    transfer_workers: Vec<Uuid>,
    scan_interval_minutes: i32,
    include_extensions_csv: String,
    exclude_patterns_csv: String,
    min_file_bytes: i64,
    output_policy: LibraryOutputPolicy,
    default_priority: i32,
}

fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn join_csv(items: &[String]) -> AppResult<String> {
    let mut cleaned = Vec::with_capacity(items.len());
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().any(char::is_control) || trimmed.contains(',') {
            return Err(AppError::Invalid);
        }
        if !cleaned
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(trimmed))
        {
            cleaned.push(trimmed.to_owned());
        }
    }
    Ok(cleaned.join(","))
}

pub fn normalize_library_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    let count = name.chars().count();
    if count == 0 || count > LIBRARY_NAME_MAX || name.chars().any(char::is_control) {
        return Err(AppError::Invalid);
    }
    Ok(name.to_owned())
}

pub fn normalize_flow_id(flow_id: Option<String>) -> Option<String> {
    flow_id.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

pub fn normalize_worker_path(path: &str) -> AppResult<String> {
    let mut path = path.trim().to_owned();
    if path.chars().any(char::is_control) || path.contains('\\') {
        return Err(AppError::Invalid);
    }
    while path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    if !path.starts_with('/') || path.chars().count() > LIBRARY_PATH_MAX {
        return Err(AppError::Invalid);
    }
    let mut segments = path.split('/');
    if segments.next() != Some("") {
        return Err(AppError::Invalid);
    }
    if segments.any(|segment| segment.is_empty() || segment == "." || segment == "..") {
        return Err(AppError::Invalid);
    }
    Ok(path)
}

pub fn prepare_workers(workers: Vec<LibraryWorkerPath>) -> AppResult<Vec<LibraryWorkerPath>> {
    let mut seen = HashSet::with_capacity(workers.len());
    let mut prepared = Vec::with_capacity(workers.len());
    for entry in workers {
        let path = normalize_worker_path(&entry.local_path)?;
        if !seen.insert((entry.worker_id, path.clone())) {
            return Err(AppError::Invalid);
        }
        prepared.push(LibraryWorkerPath {
            worker_id: entry.worker_id,
            local_path: path,
            source_key: entry
                .source_key
                .map(|key| {
                    let key = key.trim();
                    if key.len() > LIBRARY_NAME_MAX || key.chars().any(char::is_control) {
                        return Err(AppError::Invalid);
                    }
                    Ok(key.to_owned())
                })
                .transpose()?
                .filter(|key| !key.is_empty()),
            access: entry.access,
        });
    }
    let mut per_host = HashSet::new();
    if prepared.iter().any(|folder| {
        folder
            .source_key
            .as_ref()
            .is_some_and(|key| !per_host.insert((folder.worker_id, key)))
    }) {
        return Err(AppError::Invalid);
    }
    Ok(prepared)
}

/// Valide les workers « par transfert » : ni doublon, ni centrale, ni worker déjà direct.
pub fn prepare_transfer_workers(
    transfer_workers: Vec<Uuid>,
    direct: &[LibraryWorkerPath],
) -> AppResult<Vec<Uuid>> {
    if transfer_workers.is_empty() {
        return Ok(transfer_workers);
    }
    let mut seen = HashSet::with_capacity(transfer_workers.len());
    for id in &transfer_workers {
        if id.is_nil() || !seen.insert(*id) || direct.iter().any(|folder| folder.worker_id == *id) {
            return Err(AppError::Invalid);
        }
    }
    Ok(transfer_workers)
}

async fn prepare(pool: &SqlitePool, write: LibraryWrite) -> AppResult<PreparedLibrary> {
    let name = normalize_library_name(&write.name)?;
    let flow_id = normalize_flow_id(write.flow_id);
    if let Some(ref id) = flow_id {
        match super::get_flow(pool, id).await {
            Ok(_) => {}
            Err(AppError::NotFound) => return Err(AppError::Invalid),
            Err(err) => return Err(err),
        }
    }
    let workers = prepare_workers(write.workers)?;
    for entry in &workers {
        if !entry.worker_id.is_nil() && !super::worker_exists(pool, entry.worker_id).await? {
            return Err(AppError::Invalid);
        }
    }
    let transfer_workers = prepare_transfer_workers(write.transfer_workers, &workers)?;
    for id in &transfer_workers {
        if !super::worker_exists(pool, *id).await? {
            return Err(AppError::Invalid);
        }
    }
    if !(0..=MAX_SCAN_INTERVAL_MINUTES).contains(&write.scan_interval_minutes) {
        return Err(AppError::Invalid);
    }
    if !(JOB_PRIORITY_MIN..=JOB_PRIORITY_MAX).contains(&write.default_priority) {
        return Err(AppError::Invalid);
    }
    let min_file_bytes = i64::try_from(write.min_file_bytes).map_err(|_| AppError::Invalid)?;
    let include_extensions_csv = join_csv(&write.include_extensions)?;
    let exclude_patterns_csv = join_csv(&write.exclude_patterns)?;
    Ok(PreparedLibrary {
        name,
        flow_id,
        workers,
        transfer_workers,
        scan_interval_minutes: write.scan_interval_minutes,
        include_extensions_csv,
        exclude_patterns_csv,
        min_file_bytes,
        output_policy: write.output_policy,
        default_priority: write.default_priority,
    })
}

/// `default_priority` est lue à part (requête runtime) : les macros de `LibraryRow` l'ignorent.
fn assemble(
    row: LibraryRow,
    workers: Vec<LibraryWorkerPath>,
    transfer_workers: Vec<Uuid>,
    default_priority: i32,
) -> Library {
    Library {
        id: row.id,
        name: row.name,
        flow_id: row.flow_id,
        workers,
        transfer_workers,
        scan_interval_minutes: row.scan_interval_minutes.max(0),
        last_scanned_at: row.last_scanned_at,
        include_extensions: split_csv(&row.include_extensions),
        exclude_patterns: split_csv(&row.exclude_patterns),
        min_file_bytes: row.min_file_bytes.max(0) as u64,
        output_policy: row.output_policy,
        default_priority,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Priorité par défaut d'une bibliothèque (colonne lue hors macro).
async fn default_priority_of(pool: &SqlitePool, id: Uuid) -> AppResult<i32> {
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT default_priority FROM libraries WHERE id=?1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .unwrap_or(LIBRARY_DEFAULT_JOB_PRIORITY),
    )
}

/// Enregistre la priorité par défaut, dans la transaction d'écriture de la bibliothèque.
async fn save_default_priority(
    conn: &mut sqlx::sqlite::SqliteConnection,
    id: Uuid,
    default_priority: i32,
) -> AppResult<()> {
    sqlx::query("UPDATE libraries SET default_priority=?2 WHERE id=?1")
        .bind(id)
        .bind(default_priority)
        .execute(conn)
        .await?;
    Ok(())
}

async fn workers_of(
    executor: impl sqlx::SqliteExecutor<'_>,
    id: Uuid,
) -> AppResult<Vec<LibraryWorkerPath>> {
    let rows = sqlx::query_as::<_, LibraryWorkerRow>(
        "SELECT library_id, worker_id, local_path, access, source_key FROM library_folders WHERE library_id=?1 ORDER BY local_path",
    ).bind(id)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| LibraryWorkerPath {
            worker_id: row.worker_id.unwrap_or_default(),
            local_path: row.local_path,
            source_key: row.source_key,
            access: row.access,
        })
        .collect())
}

async fn transfer_workers_of(
    executor: impl sqlx::SqliteExecutor<'_>,
    id: Uuid,
) -> AppResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT worker_id FROM library_transfer_workers WHERE library_id=?1 ORDER BY worker_id",
    )
    .bind(id)
    .fetch_all(executor)
    .await?)
}

async fn insert_transfer_workers(
    conn: &mut sqlx::sqlite::SqliteConnection,
    library_id: Uuid,
    transfer_workers: &[Uuid],
) -> AppResult<()> {
    for worker_id in transfer_workers {
        sqlx::query("INSERT INTO library_transfer_workers (library_id, worker_id) VALUES (?1, ?2)")
            .bind(library_id)
            .bind(worker_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn insert_workers(
    conn: &mut sqlx::sqlite::SqliteConnection,
    library_id: Uuid,
    workers: &[LibraryWorkerPath],
) -> AppResult<()> {
    for entry in workers {
        sqlx::query(
            "INSERT INTO library_folders (library_id, worker_id, local_path, access, source_key) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(library_id)
        .bind((!entry.worker_id.is_nil()).then_some(entry.worker_id))
        .bind(&entry.local_path)
        .bind(entry.access)
        .bind(&entry.source_key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn list_libraries(pool: &SqlitePool) -> AppResult<Vec<Library>> {
    let rows = sqlx::query_as!(
        LibraryRow,
        r#"SELECT id AS "id!: Uuid", name, flow_id, scan_interval_minutes AS "scan_interval_minutes: i32", last_scanned_at AS "last_scanned_at: chrono::DateTime<chrono::Utc>", include_extensions, exclude_patterns, min_file_bytes, output_policy AS "output_policy: LibraryOutputPolicy", created_at AS "created_at: chrono::DateTime<chrono::Utc>", updated_at AS "updated_at: chrono::DateTime<chrono::Utc>" FROM libraries ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;
    let worker_rows = sqlx::query_as::<_, LibraryWorkerRow>(
        "SELECT library_id, worker_id, local_path, access, source_key FROM library_folders ORDER BY local_path",
    )
    .fetch_all(pool)
    .await?;
    let transfer_rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT library_id, worker_id FROM library_transfer_workers ORDER BY worker_id",
    )
    .fetch_all(pool)
    .await?;
    let mut priorities: HashMap<Uuid, i32> =
        sqlx::query_as::<_, (Uuid, i32)>("SELECT id, default_priority FROM libraries")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let mut transfer_by_library: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (library_id, worker_id) in transfer_rows {
        transfer_by_library
            .entry(library_id)
            .or_default()
            .push(worker_id);
    }
    let mut by_library: HashMap<Uuid, Vec<LibraryWorkerPath>> = HashMap::new();
    for entry in worker_rows {
        by_library
            .entry(entry.library_id)
            .or_default()
            .push(LibraryWorkerPath {
                worker_id: entry.worker_id.unwrap_or_default(),
                local_path: entry.local_path,
                source_key: entry.source_key,
                access: entry.access,
            });
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let workers = by_library.remove(&row.id).unwrap_or_default();
            let transfer_workers = transfer_by_library.remove(&row.id).unwrap_or_default();
            let default_priority = priorities
                .remove(&row.id)
                .unwrap_or(LIBRARY_DEFAULT_JOB_PRIORITY);
            assemble(row, workers, transfer_workers, default_priority)
        })
        .collect())
}

pub async fn get_library(pool: &SqlitePool, id: Uuid) -> AppResult<Library> {
    let row = sqlx::query_as!(
        LibraryRow,
        r#"SELECT id AS "id!: Uuid", name, flow_id, scan_interval_minutes AS "scan_interval_minutes: i32", last_scanned_at AS "last_scanned_at: chrono::DateTime<chrono::Utc>", include_extensions, exclude_patterns, min_file_bytes, output_policy AS "output_policy: LibraryOutputPolicy", created_at AS "created_at: chrono::DateTime<chrono::Utc>", updated_at AS "updated_at: chrono::DateTime<chrono::Utc>" FROM libraries WHERE id=?1"#,
        id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let workers = workers_of(pool, id).await?;
    let transfer_workers = transfer_workers_of(pool, id).await?;
    let default_priority = default_priority_of(pool, id).await?;
    Ok(assemble(row, workers, transfer_workers, default_priority))
}

pub async fn insert_library(pool: &SqlitePool, write: LibraryWrite) -> AppResult<Library> {
    let prepared = prepare(pool, write).await?;
    let id = Uuid::new_v4();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as!(
        LibraryRow,
        r#"INSERT INTO libraries (id, name, flow_id, scan_interval_minutes, include_extensions, exclude_patterns, min_file_bytes, output_policy) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id AS "id!: Uuid", name, flow_id, scan_interval_minutes AS "scan_interval_minutes: i32", last_scanned_at AS "last_scanned_at: chrono::DateTime<chrono::Utc>", include_extensions, exclude_patterns, min_file_bytes, output_policy AS "output_policy: LibraryOutputPolicy", created_at AS "created_at: chrono::DateTime<chrono::Utc>", updated_at AS "updated_at: chrono::DateTime<chrono::Utc>""#,
        id,
        prepared.name,
        prepared.flow_id,
        prepared.scan_interval_minutes,
        prepared.include_extensions_csv,
        prepared.exclude_patterns_csv,
        prepared.min_file_bytes,
        prepared.output_policy,
    )
    .fetch_one(&mut *tx)
    .await?;
    insert_workers(&mut tx, id, &prepared.workers).await?;
    insert_transfer_workers(&mut tx, id, &prepared.transfer_workers).await?;
    save_default_priority(&mut tx, id, prepared.default_priority).await?;
    tx.commit().await?;
    Ok(assemble(
        row,
        prepared.workers,
        prepared.transfer_workers,
        prepared.default_priority,
    ))
}

pub async fn update_library(
    pool: &SqlitePool,
    id: Uuid,
    write: LibraryWrite,
) -> AppResult<Library> {
    let prepared = prepare(pool, write).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as!(
        LibraryRow,
        r#"UPDATE libraries SET name=?2, flow_id=?3, scan_interval_minutes=?4, include_extensions=?5, exclude_patterns=?6, min_file_bytes=?7, output_policy=?8, updated_at=datetime('now') WHERE id=?1 RETURNING id AS "id!: Uuid", name, flow_id, scan_interval_minutes AS "scan_interval_minutes: i32", last_scanned_at AS "last_scanned_at: chrono::DateTime<chrono::Utc>", include_extensions, exclude_patterns, min_file_bytes, output_policy AS "output_policy: LibraryOutputPolicy", created_at AS "created_at: chrono::DateTime<chrono::Utc>", updated_at AS "updated_at: chrono::DateTime<chrono::Utc>""#,
        id,
        prepared.name,
        prepared.flow_id,
        prepared.scan_interval_minutes,
        prepared.include_extensions_csv,
        prepared.exclude_patterns_csv,
        prepared.min_file_bytes,
        prepared.output_policy,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(AppError::NotFound);
    };
    sqlx::query("DELETE FROM library_folders WHERE library_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM library_transfer_workers WHERE library_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_workers(&mut tx, id, &prepared.workers).await?;
    insert_transfer_workers(&mut tx, id, &prepared.transfer_workers).await?;
    save_default_priority(&mut tx, id, prepared.default_priority).await?;
    tx.commit().await?;
    Ok(assemble(
        row,
        prepared.workers,
        prepared.transfer_workers,
        prepared.default_priority,
    ))
}

pub async fn delete_library(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM library_transfer_workers WHERE library_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM library_folders WHERE library_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let affected = sqlx::query!("DELETE FROM libraries WHERE id=?1", id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

/// Chemin média local déclaré pour un couple `(bibliothèque, worker)`, utilisé pour la traduction `*arr`.
pub async fn library_worker_local_path(
    pool: &SqlitePool,
    library_id: Uuid,
    worker_id: Uuid,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT CASE WHEN count(*) = 1 THEN min(local_path) END FROM library_folders WHERE library_id=?1 AND worker_id=?2",
    )
    .bind(library_id)
    .bind(worker_id)
    .fetch_one(pool)
    .await?)
}

pub async fn library_worker_local_path_for_job(
    pool: &SqlitePool,
    job: &crate::domain::Job,
    worker_id: Uuid,
) -> AppResult<Option<String>> {
    match job.library_id {
        Some(id) => library_worker_local_path(pool, id, worker_id).await,
        None => Ok(None),
    }
}

/// Met à jour l'horodatage du dernier scan d'une bibliothèque.
pub async fn touch_library_scanned(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    sqlx::query!(
        "UPDATE libraries SET last_scanned_at = datetime('now') WHERE id = ?1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Liste les identifiants de bibliothèques ayant un scan périodique échu.
pub async fn due_scheduled_libraries(pool: &SqlitePool) -> AppResult<Vec<Uuid>> {
    let ids: Vec<Uuid> = sqlx::query_scalar!(
        r#"SELECT id AS "id!: Uuid" FROM libraries WHERE scan_interval_minutes > 0 AND flow_id IS NOT NULL AND (last_scanned_at IS NULL OR datetime(last_scanned_at, '+' || scan_interval_minutes || ' minutes') <= datetime('now'))"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rejects_blank_and_overlong_values() {
        assert_eq!(
            normalize_library_name("  Films  ").ok().as_deref(),
            Some("Films")
        );
        assert!(normalize_library_name("   ").is_err());
        assert!(normalize_library_name(&"a".repeat(LIBRARY_NAME_MAX + 1)).is_err());
        assert!(normalize_library_name("li\ngne").is_err());
    }

    #[test]
    fn path_must_be_absolute_without_parent_segments() {
        assert_eq!(
            normalize_worker_path(" /mnt/media/ ").ok().as_deref(),
            Some("/mnt/media")
        );
        assert_eq!(
            normalize_worker_path("/mnt/media/").ok().as_deref(),
            Some("/mnt/media")
        );
        assert!(normalize_worker_path("media").is_err());
        assert!(normalize_worker_path("/mnt/../etc").is_err());
        assert!(normalize_worker_path("/").is_err());
        assert!(normalize_worker_path("/mnt//media").is_err());
        assert!(normalize_flow_id(Some("  ".into())).is_none());
        assert_eq!(
            normalize_flow_id(Some(" flow.a ".into())).as_deref(),
            Some("flow.a")
        );
    }

    #[test]
    fn the_same_host_can_mount_distinct_paths() {
        let id = Uuid::nil();
        let workers = vec![
            LibraryWorkerPath {
                source_key: None,
                worker_id: id,
                local_path: "/mnt/a".into(),
                access: LibraryAccess::Read,
            },
            LibraryWorkerPath {
                source_key: None,
                worker_id: id,
                local_path: "/mnt/b".into(),
                access: LibraryAccess::ReadWrite,
            },
        ];
        assert_eq!(
            prepare_workers(workers.clone())
                .map(|paths| paths.len())
                .ok(),
            Some(2)
        );
        let mut duplicate = workers;
        duplicate[1].local_path = "/mnt/a/".into();
        assert!(prepare_workers(duplicate).is_err());
    }
}
