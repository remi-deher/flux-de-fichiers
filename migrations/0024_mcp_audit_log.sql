-- Journal d'audit des appels MCP (ADR 0015) : historique complet sans file d'approbation humaine.
CREATE TABLE IF NOT EXISTS mcp_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key_id TEXT NOT NULL,
    key_name TEXT NOT NULL,
    role TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    target_summary TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('ok', 'error', 'forbidden', 'rate_limited')),
    error_message TEXT,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS mcp_audit_log_recent_idx
    ON mcp_audit_log (created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS mcp_audit_log_key_idx
    ON mcp_audit_log (key_id, created_at DESC);
