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

    // 1. Lister les flux par défaut auto-amorcés par la migration
    let req = Request::builder()
        .uri("/api/v1/flows")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), 16 * 1024).await?;
    let flows_list: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    assert!(flows_list.len() >= 3, "au moins 3 flux par défaut");

    // 2. Créer un flux personnalisé
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

    // 3. Lire le flux personnalisé créé
    let req = Request::builder()
        .uri("/api/v1/flows/flow.custom.test")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Tentative de suppression du flux par défaut interdit (409 Conflict)
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/flows/flow.default.mkv")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // 5. Suppression du flux personnalisé autorisé (204 No Content)
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/flows/flow.custom.test")
        .header("authorization", "Bearer test-admin-key")
        .body(Body::empty())?;
    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    Ok(())
}
