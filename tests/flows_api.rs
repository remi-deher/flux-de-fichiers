use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
    flows::{self, ConditionConfig},
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

async fn test_app() -> anyhow::Result<axum::Router> {
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: "var/config".into(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key".into(),
        worker_key: "test-worker-key".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    let pool = SqlitePoolOptions::new()
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    Ok(router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    }))
}

#[test]
fn test_condition_evaluation() {
    let media_info = serde_json::json!({
        "tracks": [
            {
                "type": "video",
                "codec": "HEVC",
                "properties": { "pixel_dimensions": "1920x1080" }
            },
            {
                "type": "audio",
                "codec": "AAC",
                "properties": { "language": "fre" }
            }
        ]
    });

    let cond_hevc = ConditionConfig {
        property: "video.codec".into(),
        operator: "eq".into(),
        target: "hevc".into(),
    };
    assert!(flows::evaluate_condition(&cond_hevc, &media_info));

    let cond_h264 = ConditionConfig {
        property: "video.codec".into(),
        operator: "eq".into(),
        target: "h264".into(),
    };
    assert!(!flows::evaluate_condition(&cond_h264, &media_info));

    let cond_lang_fre = ConditionConfig {
        property: "audio.language".into(),
        operator: "contains".into(),
        target: "fre".into(),
    };
    assert!(flows::evaluate_condition(&cond_lang_fre, &media_info));

    let cond_lang_eng = ConditionConfig {
        property: "audio.language".into(),
        operator: "eq".into(),
        target: "eng".into(),
    };
    assert!(!flows::evaluate_condition(&cond_lang_eng, &media_info));
}

#[tokio::test]
async fn test_flows_crud_and_default_protection() -> anyhow::Result<()> {
    let app = test_app().await?;

    // 1. Démarrage en mode Bare Core : aucun flux n'est pré-embarqué par défaut
    let req = Request::builder()
        .uri("/api/v1/flows")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let flows_list: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    assert_eq!(
        flows_list.len(),
        0,
        "moteur nu (Bare Core) : 0 flux par défaut"
    );

    // 2. Importation d'un flux certifié (simulant l'import depuis le catalogue de plugins)
    let default_flow = serde_json::json!({
        "name": "Remuxage Matroska propre",
        "description": "Pipeline certifié importé depuis flux-de-fichiers-plugins",
        "file_mask": "*.mkv, *.avi, *.mp4",
        "is_default": true,
        "definition": "{\"nodes\":[],\"connections\":[]}"
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/v1/flows/flow.default.mkv")
        .header("authorization", "Bearer test-admin-key")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&default_flow)?))?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Créer un second flux personnalisé
    let custom_flow = serde_json::json!({
        "name": "Flux Personnalisé Test",
        "description": "Pipeline d'essai",
        "file_mask": "*.mkv",
        "is_default": false,
        "definition": "{\"nodes\":[],\"connections\":[]}"
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/v1/flows/flow.custom.test")
        .header("authorization", "Bearer test-admin-key")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&custom_flow)?))?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Lire le flux personnalisé créé
    let req = Request::builder()
        .uri("/api/v1/flows/flow.custom.test")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // 5. Tentative de suppression du flux par défaut interdit (409 Conflict)
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/flows/flow.default.mkv")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // 6. Suppression du flux personnalisé autorisé (204 No Content)
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/flows/flow.custom.test")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    Ok(())
}

#[tokio::test]
async fn webhook_block_honors_fail_on_error() -> anyhow::Result<()> {
    use flux_de_fichiers::flows::{self, WebhookBlockConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(b"HTTP/1.1 500 Nope\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
        }
    });
    let ctx = flows::FlowContext::new(
        uuid::Uuid::nil(),
        "film.mkv".into(),
        "film.mkv".into(),
        serde_json::json!({}),
    );
    let client = reqwest::Client::new();
    let mut config = WebhookBlockConfig {
        url: format!("http://{addr}/hook"),
        method: "POST".into(),
        custom_headers: std::collections::HashMap::new(),
        fail_on_error: true,
    };
    assert!(
        flows::execute_webhook_block(&config, &ctx, &client)
            .await
            .is_err()
    );

    config.fail_on_error = false;
    config.url = "http://127.0.0.1:9/hook".into();
    assert!(
        flows::execute_webhook_block(&config, &ctx, &client)
            .await
            .is_ok()
    );
    config.fail_on_error = true;
    assert!(
        flows::execute_webhook_block(&config, &ctx, &client)
            .await
            .is_err()
    );
    Ok(())
}

async fn secrets_app() -> anyhow::Result<(axum::Router, sqlx::SqlitePool, tempfile::TempDir)> {
    let config_dir = tempfile::tempdir()?;
    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: "var".into(),
        input_dir: "var/input".into(),
        config_dir: config_dir.path().to_path_buf(),
        tools_cache_dir: "var/tools-cache".into(),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        admin_key: "test-admin-key".into(),
        worker_key: "test-worker-key".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(128);
    let app = router(AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        events_tx,
    });
    Ok((app, pool, config_dir))
}

fn webhook_definition(header_value: &str) -> String {
    serde_json::json!({
        "nodes": [
            {"id": "n1", "type": "file-input", "name": "Entrée"},
            {"id": "n2", "type": "webhook", "name": "Notifier", "config": {
                "url": "https://hook.example/x",
                "custom_headers": {"Authorization": header_value}
            }}
        ],
        "connections": [{"from": "n1", "to": "n2"}]
    })
    .to_string()
}

async fn save_webhook_flow(
    app: &axum::Router,
    header_value: &str,
) -> anyhow::Result<serde_json::Value> {
    let body = serde_json::json!({
        "name": "Flux webhook",
        "description": null,
        "file_mask": "*.mkv",
        "is_default": false,
        "definition": webhook_definition(header_value),
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/flows/flow.webhook")
                .header("authorization", "Bearer test-admin-key")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body)?))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 16 * 1024).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn stored_definition(pool: &sqlx::SqlitePool) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT definition FROM flows WHERE id = 'flow.webhook'")
            .fetch_one(pool)
            .await?,
    )
}

#[tokio::test]
async fn test_webhook_headers_are_encrypted_at_rest_and_masked_by_the_api() -> anyhow::Result<()> {
    let (app, pool, _dir) = secrets_app().await?;
    let saved = save_webhook_flow(&app, "Bearer SECRET-HEADER").await?;
    let returned = saved["definition"].as_str().unwrap_or_default();
    assert!(!returned.contains("SECRET-HEADER"));
    assert!(returned.contains("Authorization"));

    let raw = stored_definition(&pool).await?;
    assert!(!raw.contains("SECRET-HEADER"));
    assert!(raw.contains("enc:v1:"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/flows/flow.webhook")
                .header("authorization", "Bearer test-admin-key")
                .body(Body::empty())?,
        )
        .await?;
    let bytes = to_bytes(response.into_body(), 16 * 1024).await?;
    assert!(!String::from_utf8_lossy(&bytes).contains("SECRET-HEADER"));
    Ok(())
}

#[tokio::test]
async fn test_resaving_a_masked_flow_keeps_the_stored_header() -> anyhow::Result<()> {
    let (app, pool, dir) = secrets_app().await?;
    save_webhook_flow(&app, "Bearer SECRET-HEADER").await?;
    let first = stored_definition(&pool).await?;

    save_webhook_flow(&app, "").await?;
    assert_eq!(stored_definition(&pool).await?, first);

    let secrets = flux_de_fichiers::secrets::SecretBox::open(dir.path())?;
    let stored: serde_json::Value = serde_json::from_str(&first)?;
    let encrypted = stored["nodes"][1]["config"]["custom_headers"]["Authorization"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(secrets.decrypt(encrypted)?, "Bearer SECRET-HEADER");
    Ok(())
}
