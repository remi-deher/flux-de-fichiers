//! Blocs d'intégration Sonarr et Radarr : rescan et renommage, avec les réglages du plugin (ADR 0009).
//!
//! L'URL, la clé d'API (chiffrée au repos) et la correspondance de chemins viennent des réglages du
//! plugin ; le bloc ne porte que sa politique d'échec. Le fichier visé est le média source, traduit du
//! chemin vu par ce central vers le chemin vu par Sonarr ou Radarr.

use crate::{
    repository,
    secrets::{SecretBox, is_encrypted},
};
use anyhow::{Context, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

const ARR_TIMEOUT: Duration = Duration::from_secs(15);
const API_KEY_HEADER: &str = "X-Api-Key";
const COMMAND_PATH: &str = "/api/v3/command";
const RENAME_PATH: &str = "/api/v3/rename";
const RENAME_COMMAND: &str = "RenameFiles";

/// Application *arr ciblée par un bloc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrService {
    Sonarr,
    Radarr,
}

impl ArrService {
    pub const ALL: [Self; 2] = [Self::Sonarr, Self::Radarr];

    pub const fn plugin_id(self) -> &'static str {
        match self {
            Self::Sonarr => "plugin.sonarr",
            Self::Radarr => "plugin.radarr",
        }
    }

    const fn list_path(self) -> &'static str {
        match self {
            Self::Sonarr => "/api/v3/series",
            Self::Radarr => "/api/v3/movie",
        }
    }

    const fn id_param(self) -> &'static str {
        match self {
            Self::Sonarr => "seriesId",
            Self::Radarr => "movieId",
        }
    }

    const fn rescan_command(self) -> &'static str {
        match self {
            Self::Sonarr => "RescanSeries",
            Self::Radarr => "RescanMovie",
        }
    }

    const fn file_id_field(self) -> &'static str {
        match self {
            Self::Sonarr => "episodeFileId",
            Self::Radarr => "movieFileId",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrAction {
    /// Demande à l'application de relire le fichier de la série ou du film.
    Rescan,
    /// Demande à l'application d'appliquer son modèle de nommage.
    Rename,
}

/// Bloc d'intégration d'un flux : quoi faire, et que faire en cas d'échec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrBlock {
    pub service: ArrService,
    pub action: ArrAction,
    /// Un échec fait échouer la tâche ; sinon il est seulement journalisé.
    pub fail_on_error: bool,
}

/// Réglages de connexion d'un plugin *arr, secret déchiffré.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrConnection {
    pub url: String,
    pub api_key: String,
    pub path_local: String,
    pub path_remote: String,
}

impl ArrConnection {
    /// Lit les réglages enregistrés ; `None` tant que l'URL ou la clé d'API manque.
    pub fn from_settings(settings: &Value, secrets: &SecretBox, plugin_id: &str) -> Option<Self> {
        let text = |name: &str| {
            settings
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|value| !value.is_empty())
        };
        let stored_key = text("apiKey")?;
        let api_key = if is_encrypted(&stored_key) {
            secrets
                .decrypt(
                    &stored_key,
                    &crate::plugins::plugin_secret_aad(plugin_id, "apiKey"),
                )
                .ok()?
        } else {
            stored_key
        };
        Some(Self {
            url: text("url")?,
            api_key,
            path_local: text("pathMappingLocal").unwrap_or_default(),
            path_remote: text("pathMappingRemote").unwrap_or_default(),
        })
    }

    /// Chemin du fichier tel que Sonarr ou Radarr le voit.
    fn remote_path(&self, local: &Path) -> anyhow::Result<String> {
        if self.path_local.is_empty() || self.path_remote.is_empty() {
            return Ok(local.to_string_lossy().into_owned());
        }
        let relative = local.strip_prefix(&self.path_local).map_err(|_| {
            anyhow!(
                "{} n'est pas sous le chemin local {}",
                local.display(),
                self.path_local
            )
        })?;
        Ok(Path::new(&self.path_remote)
            .join(relative)
            .to_string_lossy()
            .into_owned())
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.url.trim_end_matches('/'))
    }
}

async fn get_json(
    client: &Client,
    connection: &ArrConnection,
    path: &str,
) -> anyhow::Result<Value> {
    client
        .get(connection.endpoint(path))
        .header(API_KEY_HEADER, &connection.api_key)
        .timeout(ARR_TIMEOUT)
        .send()
        .await
        .with_context(|| format!("requête vers {path} impossible"))?
        .error_for_status()
        .with_context(|| format!("{path} a refusé la requête"))?
        .json()
        .await
        .with_context(|| format!("réponse illisible de {path}"))
}

/// Série ou film dont le dossier contient le fichier (le dossier le plus précis l'emporte).
async fn find_item_id(
    service: ArrService,
    client: &Client,
    connection: &ArrConnection,
    remote_file: &str,
) -> anyhow::Result<i64> {
    let items = get_json(client, connection, service.list_path()).await?;
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let folder = item.get("path")?.as_str()?.trim_end_matches('/');
            let id = item.get("id")?.as_i64()?;
            let contains = remote_file == folder || remote_file.starts_with(&format!("{folder}/"));
            contains.then_some((folder.len(), id))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, id)| id)
        .ok_or_else(|| anyhow!("aucun élément de {:?} ne contient {remote_file}", service))
}

async fn post_command(
    client: &Client,
    connection: &ArrConnection,
    body: Value,
) -> anyhow::Result<()> {
    client
        .post(connection.endpoint(COMMAND_PATH))
        .header(API_KEY_HEADER, &connection.api_key)
        .timeout(ARR_TIMEOUT)
        .json(&body)
        .send()
        .await
        .context("commande impossible")?
        .error_for_status()
        .context("la commande a été refusée")?;
    Ok(())
}

/// Exécute un bloc d'intégration sur le fichier source.
pub async fn execute_arr_block(
    block: &ArrBlock,
    connection: &ArrConnection,
    local_file: &Path,
    client: &Client,
) -> anyhow::Result<()> {
    let service = block.service;
    let remote = connection.remote_path(local_file)?;
    let id = find_item_id(service, client, connection, &remote).await?;
    match block.action {
        ArrAction::Rescan => {
            post_command(
                client,
                connection,
                json!({"name": service.rescan_command(), service.id_param(): id}),
            )
            .await
        }
        ArrAction::Rename => {
            let preview = get_json(
                client,
                connection,
                &format!("{RENAME_PATH}?{}={id}", service.id_param()),
            )
            .await?;
            let files: Vec<i64> = preview
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item.get(service.file_id_field())?.as_i64())
                .collect();
            if files.is_empty() {
                tracing::info!(service = ?service, "rien à renommer");
                return Ok(());
            }
            post_command(
                client,
                connection,
                json!({"name": RENAME_COMMAND, service.id_param(): id, "files": files}),
            )
            .await
        }
    }
}

/// Connexions chargées pour un flux : les réglages de chaque plugin *arr configuré.
#[derive(Clone, Debug, Default)]
pub struct ArrContext {
    pub connections: HashMap<ArrService, ArrConnection>,
    /// Dossier des médias sources, tel que vu par ce central.
    pub media_root: PathBuf,
}

/// Contexte *arr sérialisable qui voyage avec le bail vers un worker distant.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrLeaseContext {
    pub connections: HashMap<ArrService, ArrConnection>,
    /// Dossier des médias sources, tel que vu par le central (traduction de chemins).
    pub media_root: PathBuf,
}

impl From<&ArrContext> for ArrLeaseContext {
    fn from(value: &ArrContext) -> Self {
        Self {
            connections: value.connections.clone(),
            media_root: value.media_root.clone(),
        }
    }
}

impl From<ArrLeaseContext> for ArrContext {
    fn from(value: ArrLeaseContext) -> Self {
        Self {
            connections: value.connections,
            media_root: value.media_root,
        }
    }
}

impl ArrContext {
    /// Lit les réglages enregistrés des plugins Sonarr et Radarr ; un plugin non configuré est absent.
    pub async fn load(pool: &SqlitePool, config_dir: &Path, media_root: &Path) -> Self {
        let mut connections = HashMap::new();
        let Ok(secrets) = SecretBox::open(config_dir) else {
            return Self {
                connections,
                media_root: media_root.to_path_buf(),
            };
        };
        for service in ArrService::ALL {
            let stored = repository::get_plugin_settings(pool, service.plugin_id())
                .await
                .ok()
                .flatten();
            let settings = stored
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
            if let Some(connection) = settings.and_then(|value| {
                ArrConnection::from_settings(&value, &secrets, service.plugin_id())
            }) {
                connections.insert(service, connection);
            }
        }
        Self {
            connections,
            media_root: media_root.to_path_buf(),
        }
    }

    pub async fn run(
        &self,
        block: &ArrBlock,
        source_name: &str,
        client: &Client,
    ) -> anyhow::Result<()> {
        let connection = self.connections.get(&block.service).ok_or_else(|| {
            anyhow!(
                "le plugin {} n'est pas configuré (URL et clé d'API)",
                block.service.plugin_id()
            )
        })?;
        execute_arr_block(
            block,
            connection,
            &self.media_root.join(source_name),
            client,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::State,
        http::HeaderMap,
        routing::{get, post},
    };
    use std::sync::{Arc, Mutex};

    type Commands = Arc<Mutex<Vec<(String, Value)>>>;

    async fn serve(
        service: ArrService,
        renameable: Vec<i64>,
    ) -> anyhow::Result<(String, Commands)> {
        let commands: Commands = Arc::new(Mutex::new(Vec::new()));
        let list = json!([
            {"id": 7, "path": "/lib/Show"},
            {"id": 8, "path": "/lib/Show Two"},
            {"id": 9, "path": "/lib"}
        ]);
        let rename: Vec<Value> = renameable
            .iter()
            .map(|id| json!({service.file_id_field(): id}))
            .collect();
        let router = Router::new()
            .route(
                service.list_path(),
                get(move || {
                    let body = list.clone();
                    async move { Json(body) }
                }),
            )
            .route(
                RENAME_PATH,
                get(move || {
                    let body = rename.clone();
                    async move { Json(body) }
                }),
            )
            .route(
                COMMAND_PATH,
                post(
                    |State(commands): State<Commands>,
                     headers: HeaderMap,
                     Json(body): Json<Value>| async move {
                        let key = headers
                            .get(API_KEY_HEADER)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .to_owned();
                        if let Ok(mut guard) = commands.lock() {
                            guard.push((key, body));
                        }
                        Json(json!({}))
                    },
                ),
            )
            .with_state(commands.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok((format!("http://{address}/"), commands))
    }

    fn connection(url: String) -> ArrConnection {
        ArrConnection {
            url,
            api_key: "CLE-ARR".into(),
            path_local: "/data".into(),
            path_remote: "/lib".into(),
        }
    }

    fn block(service: ArrService, action: ArrAction) -> ArrBlock {
        ArrBlock {
            service,
            action,
            fail_on_error: true,
        }
    }

    fn recorded(commands: &Commands) -> Vec<(String, Value)> {
        commands
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn test_sonarr_rescan_targets_the_most_specific_series_with_the_api_key()
    -> anyhow::Result<()> {
        let (url, commands) = serve(ArrService::Sonarr, vec![]).await?;
        execute_arr_block(
            &block(ArrService::Sonarr, ArrAction::Rescan),
            &connection(url),
            Path::new("/data/Show/Saison 1/ep.mkv"),
            &Client::new(),
        )
        .await?;
        assert_eq!(
            recorded(&commands),
            vec![(
                "CLE-ARR".to_owned(),
                json!({"name": "RescanSeries", "seriesId": 7})
            )]
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_radarr_rescan_uses_the_movie_commands() -> anyhow::Result<()> {
        let (url, commands) = serve(ArrService::Radarr, vec![]).await?;
        execute_arr_block(
            &block(ArrService::Radarr, ArrAction::Rescan),
            &connection(url),
            Path::new("/data/Show Two/film.mkv"),
            &Client::new(),
        )
        .await?;
        assert_eq!(
            recorded(&commands)[0].1,
            json!({"name": "RescanMovie", "movieId": 8})
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_rename_sends_the_preview_file_ids_and_skips_when_nothing_to_rename()
    -> anyhow::Result<()> {
        let (url, commands) = serve(ArrService::Sonarr, vec![99, 100]).await?;
        let sonarr = connection(url);
        execute_arr_block(
            &block(ArrService::Sonarr, ArrAction::Rename),
            &sonarr,
            Path::new("/data/Show/ep.mkv"),
            &Client::new(),
        )
        .await?;
        assert_eq!(
            recorded(&commands)[0].1,
            json!({"name": "RenameFiles", "seriesId": 7, "files": [99, 100]})
        );

        let (url, commands) = serve(ArrService::Radarr, vec![]).await?;
        execute_arr_block(
            &block(ArrService::Radarr, ArrAction::Rename),
            &connection(url),
            Path::new("/data/Show/film.mkv"),
            &Client::new(),
        )
        .await?;
        assert!(recorded(&commands).is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_files_outside_the_mapping_or_any_folder_are_errors() -> anyhow::Result<()> {
        let (url, commands) = serve(ArrService::Sonarr, vec![]).await?;
        let sonarr = connection(url);
        let rescan = block(ArrService::Sonarr, ArrAction::Rescan);
        let outside =
            execute_arr_block(&rescan, &sonarr, Path::new("/autre/ep.mkv"), &Client::new()).await;
        assert!(outside.is_err());

        let mut elsewhere = sonarr.clone();
        elsewhere.path_remote = "/ailleurs".into();
        let unknown = execute_arr_block(
            &rescan,
            &elsewhere,
            Path::new("/data/ep.mkv"),
            &Client::new(),
        )
        .await;
        assert!(unknown.is_err());
        assert!(recorded(&commands).is_empty());
        Ok(())
    }

    #[test]
    fn test_remote_path_translates_the_local_root_and_passes_through_without_mapping()
    -> anyhow::Result<()> {
        let mapped = connection("http://x".into());
        assert_eq!(
            mapped.remote_path(Path::new("/data/a/b.mkv"))?,
            "/lib/a/b.mkv"
        );
        let mut open = mapped;
        open.path_local.clear();
        assert_eq!(
            open.remote_path(Path::new("/data/a/b.mkv"))?,
            "/data/a/b.mkv"
        );
        Ok(())
    }

    #[test]
    fn arr_lease_context_roundtrips_through_json() -> Result<(), serde_json::Error> {
        let mut connections = HashMap::new();
        connections.insert(
            ArrService::Sonarr,
            ArrConnection {
                url: "http://sonarr".into(),
                api_key: "cle".into(),
                path_local: "/media".into(),
                path_remote: "/tv".into(),
            },
        );
        let lease = ArrLeaseContext {
            connections,
            media_root: PathBuf::from("/data/input"),
        };
        let context = ArrContext::from(lease.clone());
        assert_eq!(ArrLeaseContext::from(&context), lease);
        let encoded = serde_json::to_value(&lease)?;
        assert_eq!(encoded["connections"]["sonarr"]["api_key"], "cle");
        assert_eq!(encoded["media_root"], "/data/input");
        Ok(())
    }

    #[test]
    fn test_from_settings_decrypts_the_key_and_requires_url_and_key() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let sealed = secrets.encrypt(
            "CLE",
            &crate::plugins::plugin_secret_aad("plugin.sonarr", "apiKey"),
        )?;
        let settings = json!({"url": "http://arr", "apiKey": sealed, "pathMappingLocal": "/data"});
        let connection = ArrConnection::from_settings(&settings, &secrets, "plugin.sonarr")
            .ok_or_else(|| anyhow!("connexion attendue"))?;
        assert_eq!(connection.api_key, "CLE");
        assert_eq!(connection.path_remote, "");
        assert!(
            ArrConnection::from_settings(&json!({"url": "http://arr"}), &secrets, "plugin.sonarr")
                .is_none()
        );
        assert!(
            ArrConnection::from_settings(&json!({"apiKey": "x"}), &secrets, "plugin.sonarr")
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_context_reports_an_unconfigured_plugin() -> anyhow::Result<()> {
        let context = ArrContext::default();
        let result = context
            .run(
                &block(ArrService::Sonarr, ArrAction::Rescan),
                "ep.mkv",
                &Client::new(),
            )
            .await;
        let message = result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(message.contains("plugin.sonarr"));
        Ok(())
    }
}
