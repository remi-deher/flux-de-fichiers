use crate::{
    domain::{FlowRecord, UpdateFlowRequest},
    error::{AppError, AppResult},
};
use sqlx::SqlitePool;

/// Flow par défaut retenu pour une nouvelle tâche (le plus récent s'il y en avait plusieurs).
pub(super) const DEFAULT_FLOW_SQL: &str =
    "SELECT * FROM flows WHERE is_default=1 ORDER BY updated_at DESC LIMIT 1";

/// Projection d'une version figée dans la forme d'un flow, lisible après suppression du flow.
const FLOW_VERSION_SQL: &str = "SELECT v.flow_id AS id, v.name, v.description, v.file_mask, \
            COALESCE(f.is_default, 0) AS is_default, v.definition, v.version, \
            v.created_at, v.created_at AS updated_at \
     FROM flow_versions v LEFT JOIN flows f ON f.id = v.flow_id \
     WHERE v.flow_id=?1 AND v.version=?2";

/// Table qui stocke une définition de flow dont les secrets peuvent être rechiffrés.
pub struct DefinitionStore {
    /// Renvoie `(flow_id, rowid, definition)`.
    pub select: &'static str,
    /// Paramètres : `?1` définition, `?2` rowid.
    pub update: &'static str,
}

/// Définition courante et versions figées : toutes deux portent des secrets scellés.
pub const DEFINITION_STORES: [DefinitionStore; 2] = [
    DefinitionStore {
        select: "SELECT id, rowid, definition FROM flows",
        update: "UPDATE flows SET definition=?1 WHERE rowid=?2",
    },
    DefinitionStore {
        select: "SELECT flow_id, rowid, definition FROM flow_versions",
        update: "UPDATE flow_versions SET definition=?1 WHERE rowid=?2",
    },
];

pub async fn list_flows(pool: &SqlitePool) -> AppResult<Vec<FlowRecord>> {
    let flows =
        sqlx::query_as::<_, FlowRecord>("SELECT * FROM flows ORDER BY is_default DESC, name ASC")
            .fetch_all(pool)
            .await?;
    Ok(flows)
}

pub async fn get_flow(pool: &SqlitePool, id: &str) -> AppResult<FlowRecord> {
    sqlx::query_as::<_, FlowRecord>("SELECT * FROM flows WHERE id=?1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

/// Version figée d'un flow, telle qu'enregistrée au moment où une tâche l'a retenue.
pub async fn get_flow_version(pool: &SqlitePool, id: &str, version: i64) -> AppResult<FlowRecord> {
    sqlx::query_as::<_, FlowRecord>(FLOW_VERSION_SQL)
        .bind(id)
        .bind(version)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn default_flow(pool: &SqlitePool) -> AppResult<Option<FlowRecord>> {
    let flow = sqlx::query_as::<_, FlowRecord>(DEFAULT_FLOW_SQL)
        .fetch_optional(pool)
        .await?;
    Ok(flow)
}

/// Contenu versionné d'un flow ; `is_default` n'en fait pas partie.
#[derive(PartialEq)]
struct VersionContent {
    name: String,
    description: Option<String>,
    file_mask: String,
    definition: String,
}

impl From<&UpdateFlowRequest> for VersionContent {
    fn from(payload: &UpdateFlowRequest) -> Self {
        Self {
            name: payload.name.clone(),
            description: payload.description.clone(),
            file_mask: payload.file_mask.clone(),
            definition: payload.definition.clone(),
        }
    }
}

/// Enregistre un flow. Un contenu changé crée la version suivante ; un contenu identique
/// garde la version courante. Les tâches déjà créées gardent leur version.
pub async fn save_flow(
    pool: &SqlitePool,
    id: &str,
    payload: &UpdateFlowRequest,
) -> AppResult<FlowRecord> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let latest: Option<(i64, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT version, name, description, file_mask, definition FROM flow_versions \
         WHERE flow_id=?1 ORDER BY version DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let latest = latest.map(|(version, name, description, file_mask, definition)| {
        let content = VersionContent {
            name,
            description,
            file_mask,
            definition,
        };
        (version, content)
    });
    let content = VersionContent::from(payload);
    let version = match latest {
        Some((version, previous)) if previous == content => version,
        latest => {
            let next = latest.map_or(0, |(version, _)| version) + 1;
            sqlx::query(
                "INSERT INTO flow_versions \
                 (flow_id, version, name, description, file_mask, definition) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(id)
            .bind(next)
            .bind(&content.name)
            .bind(&content.description)
            .bind(&content.file_mask)
            .bind(&content.definition)
            .execute(&mut *tx)
            .await?;
            next
        }
    };
    if payload.is_default {
        sqlx::query("UPDATE flows SET is_default=0")
            .execute(&mut *tx)
            .await?;
    }
    let flow = sqlx::query_as::<_, FlowRecord>(
        "INSERT INTO flows \
         (id, name, description, file_mask, is_default, definition, version, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now')) \
         ON CONFLICT(id) DO UPDATE SET \
            name=excluded.name, \
            description=excluded.description, \
            file_mask=excluded.file_mask, \
            is_default=excluded.is_default, \
            definition=excluded.definition, \
            version=excluded.version, \
            updated_at=datetime('now') \
         RETURNING *",
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.file_mask)
    .bind(payload.is_default)
    .bind(&payload.definition)
    .bind(version)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(flow)
}

/// Supprime un flow ; ses versions restent pour les tâches qui les ont retenues.
pub async fn delete_flow(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let is_default: Option<bool> = sqlx::query_scalar("SELECT is_default FROM flows WHERE id=?1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    match is_default {
        Some(true) => Err(AppError::Conflict),
        Some(false) => {
            sqlx::query(
                "UPDATE libraries SET flow_id=NULL, updated_at=datetime('now') WHERE flow_id=?1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM flows WHERE id=?1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        }
        None => Err(AppError::NotFound),
    }
}
