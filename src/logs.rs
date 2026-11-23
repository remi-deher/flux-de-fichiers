//! Journal applicatif : lecture bornée des fichiers JSON écrits par `tracing` (`LOG_DIR`).
//!
//! Les lignes sont lues depuis la fin des fichiers les plus récents, sans jamais charger un
//! fichier entier (un jour de journal dépasse 40 Mio).

use crate::{
    api::{AppState, ManageApiKeysAuth},
    error::{AppError, AppResult},
};
use axum::{
    Json,
    extract::{Query, State},
};
use forgearr_shared::logs::{
    DEFAULT_LOGS_LIMIT, LogEntry, LogField, LogLevel, LogsQuery, LogsResponse, MAX_LOGS_LIMIT,
};
use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const LOG_DIR_ENV: &str = "LOG_DIR";
pub const DEFAULT_LOG_DIR: &str = "var/log";
/// Préfixe des fichiers journaliers (`flux.jsonl.2026-11-18`).
pub const LOG_FILE_PREFIX: &str = "flux.jsonl";
const TAIL_BYTES_PER_FILE: u64 = 4 * 1024 * 1024;
const MAX_SCANNED_FILES: usize = 4;
const MAX_FIELD_CHARS: usize = 600;
const MODULE_SEPARATOR: &str = "::";
const MESSAGE_FIELD: &str = "message";

/// Dossier des journaux, partagé avec l'initialisation de `tracing`.
pub fn log_dir() -> PathBuf {
    std::env::var(LOG_DIR_ENV)
        .unwrap_or_else(|_| DEFAULT_LOG_DIR.into())
        .into()
}

/// Fichiers journaliers, du plus récent au plus ancien (le suffixe est une date ISO).
fn log_files_newest_first(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(LOG_FILE_PREFIX))
        })
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    files
}

/// Lit la fin d'un fichier ; renvoie le texte et `true` si du contenu plus ancien a été laissé.
fn read_tail(path: &Path, max_bytes: u64) -> std::io::Result<(String, bool)> {
    let mut file = File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start == 0 {
        return Ok((text, false));
    }
    // Le début de la fenêtre coupe probablement une ligne : on la jette.
    let whole_lines = text
        .split_once('\n')
        .map_or_else(String::new, |(_, rest)| rest.to_owned());
    Ok((whole_lines, true))
}

fn clip(value: &str) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(MAX_FIELD_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn field_text(value: &Value) -> String {
    match value {
        Value::String(text) => clip(text),
        other => clip(&other.to_string()),
    }
}

/// Convertit une ligne JSON de `tracing` ; les lignes illisibles sont ignorées.
fn parse_line(line: &str) -> Option<LogEntry> {
    let json: Value = serde_json::from_str(line.trim()).ok()?;
    let level = LogLevel::parse(json.get("level")?.as_str()?)?;
    let fields = json.get("fields").and_then(Value::as_object);
    let message = fields
        .and_then(|map| map.get(MESSAGE_FIELD))
        .map(field_text)
        .unwrap_or_default();
    let target = json
        .get("target")
        .and_then(Value::as_str)
        .map(|full| full.rsplit(MODULE_SEPARATOR).next().unwrap_or(full))
        .unwrap_or_default()
        .to_owned();
    let fields = fields
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.as_str() != MESSAGE_FIELD)
        .map(|(name, value)| LogField {
            name: name.clone(),
            value: field_text(value),
        })
        .collect();
    Some(LogEntry {
        timestamp: json
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        level,
        target,
        message,
        fields,
    })
}

fn is_kept(entry: &LogEntry, minimum: Option<LogLevel>, needle: Option<&str>) -> bool {
    if minimum.is_some_and(|level| entry.level < level) {
        return false;
    }
    needle.is_none_or(|text| {
        let contains = |haystack: &str| haystack.to_lowercase().contains(text);
        contains(&entry.message)
            || contains(&entry.target)
            || entry
                .fields
                .iter()
                .any(|field| contains(&field.name) || contains(&field.value))
    })
}

/// Lignes les plus récentes d'abord, filtrées, dans la limite demandée.
pub fn read_entries(dir: &Path, query: &LogsQuery) -> LogsResponse {
    read_entries_bounded(dir, query, TAIL_BYTES_PER_FILE)
}

fn read_entries_bounded(dir: &Path, query: &LogsQuery, tail_bytes: u64) -> LogsResponse {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LOGS_LIMIT)
        .clamp(1, MAX_LOGS_LIMIT);
    let needle = query
        .search
        .as_deref()
        .map(|text| text.trim().to_lowercase())
        .filter(|text| !text.is_empty());
    let files = log_files_newest_first(dir);
    let mut truncated = files.len() > MAX_SCANNED_FILES;
    let mut entries = Vec::new();
    for path in files.iter().take(MAX_SCANNED_FILES) {
        let Ok((text, cut)) = read_tail(path, tail_bytes) else {
            continue;
        };
        for line in text.lines().rev() {
            let Some(entry) = parse_line(line) else {
                continue;
            };
            if is_kept(&entry, query.level, needle.as_deref()) {
                entries.push(entry);
                if entries.len() >= limit {
                    return LogsResponse {
                        entries,
                        truncated: true,
                    };
                }
            }
        }
        truncated |= cut;
    }
    LogsResponse { entries, truncated }
}

/// Journal du central (`GET /api/v1/logs`), réservé aux administrateurs.
pub async fn list(
    State(_state): State<AppState>,
    _auth: ManageApiKeysAuth,
    Query(query): Query<LogsQuery>,
) -> AppResult<Json<LogsResponse>> {
    let response = tokio::task::spawn_blocking(move || read_entries(&log_dir(), &query))
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::router, api_keys, config::Config};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use forgearr_shared::Role;
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn line(level: &str, target: &str, message: &str, extra: &Value) -> String {
        json!({
            "timestamp": "2026-11-18T17:10:06.000000Z",
            "level": level,
            "target": target,
            "fields": { "message": message, "provider": extra },
        })
        .to_string()
    }

    fn write_log(dir: &Path, name: &str, lines: &[String]) -> anyhow::Result<()> {
        std::fs::write(dir.join(name), lines.join("\n") + "\n")?;
        Ok(())
    }

    #[test]
    fn parse_line_extracts_level_module_message_and_fields() {
        let raw = line(
            "WARN",
            "forgearr::ai",
            "échec d'appel LLM",
            &json!("chatgpt"),
        );
        let entry = parse_line(&raw);
        assert_eq!(entry.as_ref().map(|e| e.level), Some(LogLevel::Warn));
        assert_eq!(entry.as_ref().map(|e| e.target.as_str()), Some("ai"));
        assert_eq!(
            entry.as_ref().map(|e| e.message.as_str()),
            Some("échec d'appel LLM")
        );
        assert_eq!(
            entry.map(|e| e.fields),
            Some(vec![LogField {
                name: "provider".into(),
                value: "chatgpt".into()
            }])
        );
        assert!(parse_line("pas du json").is_none());
        assert!(parse_line(r#"{"level":"FATAL"}"#).is_none());
    }

    #[test]
    fn entries_are_newest_first_across_files_and_filtered() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        write_log(
            dir.path(),
            "flux.jsonl.2026-11-17",
            &[
                line("INFO", "forgearr::a", "ancien", &json!("x")),
                "ligne corrompue".into(),
            ],
        )?;
        write_log(
            dir.path(),
            "flux.jsonl.2026-11-18",
            &[
                line("INFO", "forgearr::a", "premier", &json!("x")),
                line("ERROR", "forgearr::ai", "Échec", &json!("ChatGPT")),
            ],
        )?;
        std::fs::write(dir.path().join("autre.txt"), "ignoré")?;

        let all = read_entries(dir.path(), &LogsQuery::default());
        let messages: Vec<_> = all.entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, ["Échec", "premier", "ancien"]);
        assert!(!all.truncated);

        let errors = read_entries(
            dir.path(),
            &LogsQuery {
                level: Some(LogLevel::Warn),
                ..LogsQuery::default()
            },
        );
        assert_eq!(errors.entries.len(), 1);

        // La recherche couvre message, module et valeur des champs, sans casse.
        for needle in ["échec", "AI", "chatgpt"] {
            let found = read_entries(
                dir.path(),
                &LogsQuery {
                    search: Some(needle.into()),
                    ..LogsQuery::default()
                },
            );
            assert_eq!(found.entries.len(), 1, "recherche « {needle} »");
        }
        Ok(())
    }

    #[test]
    fn limit_truncates_and_missing_directory_is_empty() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let lines: Vec<String> = (0..5)
            .map(|n| line("INFO", "forgearr::a", &format!("m{n}"), &json!("x")))
            .collect();
        write_log(dir.path(), "flux.jsonl.2026-11-18", &lines)?;
        let limited = read_entries(
            dir.path(),
            &LogsQuery {
                limit: Some(2),
                ..LogsQuery::default()
            },
        );
        assert_eq!(limited.entries.len(), 2);
        assert_eq!(limited.entries[0].message, "m4");
        assert!(limited.truncated);

        let missing = read_entries(&dir.path().join("absent"), &LogsQuery::default());
        assert!(missing.entries.is_empty() && !missing.truncated);
        Ok(())
    }

    #[test]
    fn tail_window_drops_the_partial_first_line() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let lines: Vec<String> = (0..50)
            .map(|n| line("INFO", "forgearr::a", &format!("ligne-{n:02}"), &json!("x")))
            .collect();
        write_log(dir.path(), "flux.jsonl.2026-11-18", &lines)?;
        let window = read_entries_bounded(dir.path(), &LogsQuery::default(), 600);
        assert!(window.truncated);
        assert!(!window.entries.is_empty() && window.entries.len() < 50);
        // Les lignes lues sont entières et ce sont les plus récentes.
        assert_eq!(window.entries[0].message, "ligne-49");
        Ok(())
    }

    #[tokio::test]
    async fn route_is_admin_only() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        let config = Config {
            bind: ([127, 0, 0, 1], 3000).into(),
            database_url: "sqlite::memory:".into(),
            data_dir: dir.path().to_path_buf(),
            input_dir: dir.path().join("input"),
            config_dir: dir.path().to_path_buf(),
            tools_cache_dir: dir.path().join("tools"),
            plugins_dir: None,
            detected_plugins_dir: None,
            static_dir: "frontend/dist".into(),
            max_file_bytes: 2048,
            local_worker: false,
            web_origin: None,
            web_session_ttl_seconds: 28_800,
            tls: None,
            smtp: None,
        };
        let state = AppState::new(pool, Arc::new(config));
        let admin = api_keys::issue(&state.pool, "logs-admin", Role::Admin).await?;
        let viewer = api_keys::issue(&state.pool, "logs-viewer", Role::Viewer).await?;
        let app = router(state);
        let get = |raw: &str| {
            Request::builder()
                .uri("/api/v1/logs?level=warn&limit=5")
                .header("authorization", format!("Bearer {raw}"))
                .body(Body::empty())
        };
        let ok = app.clone().oneshot(get(&admin.raw)?).await?;
        assert_eq!(ok.status(), StatusCode::OK);
        let body: LogsResponse =
            serde_json::from_slice(&axum::body::to_bytes(ok.into_body(), 1 << 20).await?)?;
        assert!(body.entries.len() <= 5);
        let denied = app.oneshot(get(&viewer.raw)?).await?;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        Ok(())
    }
}
