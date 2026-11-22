//! Installation vierge dans un dossier de configuration isolé (base en mémoire).

use forgearr::{api::AppState, config::Config};
use sqlx::sqlite::SqlitePoolOptions;
use std::{path::Path, sync::Arc};

pub fn blank_config(config_dir: &Path) -> anyhow::Result<Config> {
    Ok(Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: config_dir.to_path_buf(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    })
}

pub async fn blank_state(config_dir: &Path) -> anyhow::Result<AppState> {
    let config = blank_config(config_dir)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(AppState::new(pool, Arc::new(config)))
}
