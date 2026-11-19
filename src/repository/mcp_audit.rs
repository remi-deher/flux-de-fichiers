//! Persistance du journal d'audit des actions MCP (ADR 0015).

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use forgearr_shared::mcp::{McpAuditEntry, McpAuditListResponse, McpAuditOutcome};
use sqlx::SqlitePool;

/// Nombre maximal d'entrées conservées dans `mcp_audit_log`.
pub const MCP_AUDIT_RETENTION_ROWS: i64 = 5000;
/// Nombre d'entrées renvoyées par défaut sans limite explicite.
pub const DEFAULT_AUDIT_LIMIT: i64 = 50;
/// Plafond d'une requête de lecture sur le journal MCP.
pub const MAX_AUDIT_LIMIT: i64 = 200;

/// Paramètres d'enregistrement d'une entrée dans `mcp_audit_log`.
pub struct NewMcpAuditEntry<'a> {
    pub key_id: &'a str,
    pub key_name: &'a str,
    pub role: &'a str,
    pub tool_name: &'a str,
    pub target_summary: Option<&'a str>,
    pub outcome: McpAuditOutcome,
    pub error_message: Option<&'a str>,
    pub duration_ms: i64,
}

/// Enregistre un appel MCP et applique la rétention bornée.
pub async fn record_mcp_audit(pool: &SqlitePool, entry: NewMcpAuditEntry<'_>) -> AppResult<()> {
    let outcome_str = entry.outcome.as_str();
    let duration_ms = entry.duration_ms.max(0);
    sqlx::query!(
        "INSERT INTO mcp_audit_log \
         (key_id, key_name, role, tool_name, target_summary, outcome, error_message, duration_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        entry.key_id,
        entry.key_name,
        entry.role,
        entry.tool_name,
        entry.target_summary,
        outcome_str,
        entry.error_message,
        duration_ms,
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        "DELETE FROM mcp_audit_log \
         WHERE id <= (SELECT id FROM mcp_audit_log ORDER BY id DESC LIMIT 1 OFFSET ?1)",
        MCP_AUDIT_RETENTION_ROWS,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Liste l'historique des appels MCP du plus récent au plus ancien avec filtrage optionnel.
pub async fn list_mcp_audit(
    pool: &SqlitePool,
    limit: i64,
    key_id: Option<&str>,
    outcome: Option<McpAuditOutcome>,
) -> AppResult<McpAuditListResponse> {
    let capped_limit = limit.clamp(1, MAX_AUDIT_LIMIT);
    let key_filter = key_id.map(str::trim).filter(|s| !s.is_empty());
    let outcome_filter = outcome.map(McpAuditOutcome::as_str);

    let total: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM mcp_audit_log WHERE (?1 IS NULL OR key_id = ?1) AND (?2 IS NULL OR outcome = ?2)"#,
        key_filter,
        outcome_filter,
    )
    .fetch_one(pool)
    .await?;

    let entries = sqlx::query_as!(
        McpAuditEntry,
        r#"SELECT id AS "id!: i64", key_id, key_name, role, tool_name, target_summary, outcome AS "outcome: McpAuditOutcome", error_message, duration_ms, created_at AS "created_at: DateTime<Utc>" FROM mcp_audit_log WHERE (?1 IS NULL OR key_id = ?1) AND (?2 IS NULL OR outcome = ?2) ORDER BY created_at DESC, id DESC LIMIT ?3"#,
        key_filter,
        outcome_filter,
        capped_limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(McpAuditListResponse { entries, total })
}
