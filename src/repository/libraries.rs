//! Persistance des bibliothèques déclarées et de leurs chemins par worker.

use crate::{
    domain::{
        LIBRARY_NAME_MAX, LIBRARY_PATH_MAX, Library, LibraryAccess, LibraryWorkerPath, LibraryWrite,
    },
    error::{AppError, AppResult},
};
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct LibraryRow {
    id: Uuid,
    name: String,
    flow_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct LibraryWorkerRow {
    library_id: Uuid,
    worker_id: Uuid,
    local_path: String,
    access: LibraryAccess,
}

struct PreparedLibrary {
    name: String,
    flow_id: Option<String>,
    workers: Vec<LibraryWorkerPath>,
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
    let mut seen = Vec::with_capacity(workers.len());
    let mut prepared = Vec::with_capacity(workers.len());
    for entry in workers {
        if seen.contains(&entry.worker_id) {
            return Err(AppError::Invalid);
        }
        seen.push(entry.worker_id);
        prepared.push(LibraryWorkerPath {
            worker_id: entry.worker_id,
            local_path: normalize_worker_path(&entry.local_path)?,
            access: entry.access,
        });
    }
    Ok(prepared)
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
        if !super::worker_exists(pool, entry.worker_id).await? {
            return Err(AppError::Invalid);
        }
    }
    Ok(PreparedLibrary {
        name,
        flow_id,
        workers,
    })
}

fn assemble(row: LibraryRow, workers: Vec<LibraryWorkerPath>) -> Library {
    Library {
        id: row.id,
        name: row.name,
        flow_id: row.flow_id,
        workers,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

async fn workers_of(
    executor: impl sqlx::SqliteExecutor<'_>,
    id: Uuid,
) -> AppResult<Vec<LibraryWorkerPath>> {
    let rows = sqlx::query_as::<_, LibraryWorkerRow>(
        "SELECT library_id, worker_id, local_path, access FROM library_workers \
         WHERE library_id=?1 ORDER BY local_path",
    )
    .bind(id)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| LibraryWorkerPath {
            worker_id: row.worker_id,
            local_path: row.local_path,
            access: row.access,
        })
        .collect())
}

async fn insert_workers(
    conn: &mut sqlx::sqlite::SqliteConnection,
    library_id: Uuid,
    workers: &[LibraryWorkerPath],
) -> AppResult<()> {
    for entry in workers {
        sqlx::query(
            "INSERT INTO library_workers (library_id, worker_id, local_path, access) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(library_id)
        .bind(entry.worker_id)
        .bind(&entry.local_path)
        .bind(entry.access)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn list_libraries(pool: &SqlitePool) -> AppResult<Vec<Library>> {
    let rows = sqlx::query_as::<_, LibraryRow>("SELECT * FROM libraries ORDER BY name")
        .fetch_all(pool)
        .await?;
    let worker_rows = sqlx::query_as::<_, LibraryWorkerRow>(
        "SELECT library_id, worker_id, local_path, access FROM library_workers ORDER BY local_path",
    )
    .fetch_all(pool)
    .await?;
    let mut by_library: HashMap<Uuid, Vec<LibraryWorkerPath>> = HashMap::new();
    for entry in worker_rows {
        by_library
            .entry(entry.library_id)
            .or_default()
            .push(LibraryWorkerPath {
                worker_id: entry.worker_id,
                local_path: entry.local_path,
                access: entry.access,
            });
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let workers = by_library.remove(&row.id).unwrap_or_default();
            assemble(row, workers)
        })
        .collect())
}

pub async fn get_library(pool: &SqlitePool, id: Uuid) -> AppResult<Library> {
    let row = sqlx::query_as::<_, LibraryRow>("SELECT * FROM libraries WHERE id=?1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let workers = workers_of(pool, id).await?;
    Ok(assemble(row, workers))
}

pub async fn insert_library(pool: &SqlitePool, write: LibraryWrite) -> AppResult<Library> {
    let prepared = prepare(pool, write).await?;
    let id = Uuid::new_v4();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as::<_, LibraryRow>(
        "INSERT INTO libraries (id, name, flow_id) VALUES (?1, ?2, ?3) RETURNING *",
    )
    .bind(id)
    .bind(&prepared.name)
    .bind(&prepared.flow_id)
    .fetch_one(&mut *tx)
    .await?;
    insert_workers(&mut tx, id, &prepared.workers).await?;
    tx.commit().await?;
    Ok(assemble(row, prepared.workers))
}

pub async fn update_library(
    pool: &SqlitePool,
    id: Uuid,
    write: LibraryWrite,
) -> AppResult<Library> {
    let prepared = prepare(pool, write).await?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as::<_, LibraryRow>(
        "UPDATE libraries SET name=?2, flow_id=?3, updated_at=datetime('now') \
         WHERE id=?1 RETURNING *",
    )
    .bind(id)
    .bind(&prepared.name)
    .bind(&prepared.flow_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(AppError::NotFound);
    };
    sqlx::query("DELETE FROM library_workers WHERE library_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_workers(&mut tx, id, &prepared.workers).await?;
    tx.commit().await?;
    Ok(assemble(row, prepared.workers))
}

pub async fn delete_library(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM library_workers WHERE library_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let affected = sqlx::query("DELETE FROM libraries WHERE id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
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
    fn the_same_node_cannot_be_attached_twice() {
        let id = Uuid::nil();
        let workers = vec![
            LibraryWorkerPath {
                worker_id: id,
                local_path: "/mnt/a".into(),
                access: LibraryAccess::Read,
            },
            LibraryWorkerPath {
                worker_id: id,
                local_path: "/mnt/b".into(),
                access: LibraryAccess::ReadWrite,
            },
        ];
        assert!(prepare_workers(workers).is_err());
    }
}
