use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
    plugins::PluginRepository,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tower::ServiceExt;

const TEST_ADMIN_KEY: &str = "test-admin-secret-key";
const BROADCAST_CAPACITY: usize = 128;
const MAX_TEST_FILE_BYTES: u64 = 1024;
const WEB_SESSION_TTL_SECONDS: i64 = 28_800;

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
        admin_key: TEST_ADMIN_KEY.into(),
        worker_key: "test-worker-key".into(),
        max_file_bytes: MAX_TEST_FILE_BYTES,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: WEB_SESSION_TTL_SECONDS,
        tls: None,
    };
    let pool = SqlitePoolOptions::new()
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let (events_tx, _) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
    Ok(router(AppState {
        pool,
        config: Arc::new(config),
        events_tx,
    }))
}

#[tokio::test]
async fn test_repositories_lifecycle_and_official_protection() -> anyhow::Result<()> {
    let app = test_app().await?;

    // 1. Lister les dépôts initiaux (doit contenir le catalogue officiel actif)
    let req = Request::builder()
        .uri("/api/v1/plugins/repositories")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .body(Body::empty())?;
    let res = app.clone().oneshot(req).await?;
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), 1024 * 16).await?;
    let initial_repos: Vec<PluginRepository> = serde_json::from_slice(&bytes)?;
    assert_eq!(initial_repos.len(), 1);
    assert_eq!(initial_repos[0].id, "repo.official");
    assert!(initial_repos[0].is_official);
    assert!(
        initial_repos[0]
            .index_url
            .ends_with("/dev/catalogue/index.json")
    );
    assert!(initial_repos[0].enabled);

    // 2. Ajouter un dépôt tiers avec validation d'URL
    let invalid_req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/repositories")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Dépôt Invalide",
            "indexUrl": "ftp://example.com/index.json"
        }))?))?;
    let invalid_res = app.clone().oneshot(invalid_req).await?;
    assert_eq!(invalid_res.status(), StatusCode::BAD_REQUEST);

    let wrong_suffix_req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/repositories")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "name": "Mauvais suffixe",
            "indexUrl": "https://example.com/custom/index.json"
        }))?))?;
    let wrong_suffix_res = app.clone().oneshot(wrong_suffix_req).await?;
    assert_eq!(wrong_suffix_res.status(), StatusCode::BAD_REQUEST);

    let unsafe_id_req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/repositories")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "id": "../evil",
            "name": "Identifiant dangereux",
            "indexUrl": "https://example.com/custom/catalogue/index.json"
        }))?))?;
    let unsafe_id_res = app.clone().oneshot(unsafe_id_req).await?;
    assert_eq!(unsafe_id_res.status(), StatusCode::BAD_REQUEST);

    let valid_req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/repositories")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&serde_json::json!({
            "id": "repo.custom.test",
            "name": "Dépôt Communautaire Test",
            "indexUrl": "http://127.0.0.1:9/custom/catalogue/index.json"
        }))?))?;
    let valid_res = app.clone().oneshot(valid_req).await?;
    assert_eq!(valid_res.status(), StatusCode::OK);

    // 3. Basculer l'état actif/inactif du dépôt tiers
    let toggle_req = Request::builder()
        .method("PUT")
        .uri("/api/v1/plugins/repositories/repo.custom.test/toggle")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .body(Body::empty())?;
    let toggle_res = app.clone().oneshot(toggle_req).await?;
    assert_eq!(toggle_res.status(), StatusCode::OK);

    let toggle_bytes = to_bytes(toggle_res.into_body(), 1024 * 4).await?;
    let toggle_json: serde_json::Value = serde_json::from_slice(&toggle_bytes)?;
    assert_eq!(toggle_json["enabled"], false);

    // 4. Synchroniser le dépôt
    let sync_req = Request::builder()
        .method("POST")
        .uri("/api/v1/plugins/repositories/repo.custom.test/sync")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .body(Body::empty())?;
    let sync_res = app.clone().oneshot(sync_req).await?;
    // Adresse privée : refusée par défaut, sans aucun accès réseau.
    assert_eq!(sync_res.status(), StatusCode::BAD_GATEWAY);

    // 5. Protéger le dépôt officiel contre la suppression
    let del_official_req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/plugins/repositories/repo.official")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .body(Body::empty())?;
    let del_official_res = app.clone().oneshot(del_official_req).await?;
    assert_eq!(del_official_res.status(), StatusCode::BAD_REQUEST);

    // 6. Supprimer le dépôt tiers
    let del_custom_req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/plugins/repositories/repo.custom.test")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .body(Body::empty())?;
    let del_custom_res = app.clone().oneshot(del_custom_req).await?;
    assert_eq!(del_custom_res.status(), StatusCode::OK);

    // 7. Vérifier que seul le dépôt officiel subsiste
    let list_req = Request::builder()
        .uri("/api/v1/plugins/repositories")
        .header("authorization", format!("Bearer {TEST_ADMIN_KEY}"))
        .body(Body::empty())?;
    let list_res = app.clone().oneshot(list_req).await?;
    let list_bytes = to_bytes(list_res.into_body(), 1024 * 16).await?;
    let final_repos: Vec<PluginRepository> = serde_json::from_slice(&list_bytes)?;
    assert_eq!(final_repos.len(), 1);
    assert_eq!(final_repos[0].id, "repo.official");

    Ok(())
}
