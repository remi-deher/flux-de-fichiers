mod common;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use flux_de_fichiers::{
    api::{AppState, router},
    config::Config,
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
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    Ok(router(AppState::new(pool, Arc::new(config))))
}

async fn json_of(response: axum::response::Response) -> anyhow::Result<serde_json::Value> {
    let bytes = to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn admin(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<axum::response::Response> {
    let mut builder = Request::builder().method(method).uri(uri).header(
        "authorization",
        "Bearer fdf_a0000001_0123456789abcdef0123456789abcdef0123456789abcdef",
    );
    let request = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        builder.body(Body::from(serde_json::to_vec(&body)?))?
    } else {
        builder.body(Body::empty())?
    };
    Ok(app.clone().oneshot(request).await?)
}

async fn register_worker(app: &axum::Router, name: &str) -> anyhow::Result<String> {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/workers")
        .header(
            "authorization",
            "Bearer fdf_b0000001_fedcba9876543210fedcba9876543210fedcba9876543210",
        )
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"name":"{name}"}}"#)))?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_of(response).await?;
    Ok(body["id"].as_str().unwrap_or_default().to_owned())
}

async fn save_flow(app: &axum::Router, id: &str, name: &str) -> anyhow::Result<()> {
    let response = admin(
        app,
        "PUT",
        &format!("/api/v1/flows/{id}"),
        Some(serde_json::json!({
            "name": name,
            "description": "Flux de la bibliothèque",
            "file_mask": "*.mkv",
            "is_default": false,
            "definition": "{\"nodes\":[],\"connections\":[]}"
        })),
    )
    .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn libraries_round_trip_flow_and_worker_paths() -> anyhow::Result<()> {
    let app = test_app().await?;
    let listed = admin(&app, "GET", "/api/v1/libraries", None).await?;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(json_of(listed).await?, serde_json::json!([]));

    let missing = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({
            "name": "Films",
            "flow_id": "flow.absent",
            "workers": []
        })),
    )
    .await?;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

    let relative = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({
            "name": "Films",
            "workers": [{
                "worker_id": "00000000-0000-0000-0000-000000000001",
                "local_path": "media",
                "access": "read"
            }]
        })),
    )
    .await?;
    assert_eq!(relative.status(), StatusCode::BAD_REQUEST);

    save_flow(&app, "flow.films", "Remux films").await?;
    let worker_a = register_worker(&app, "nas-a").await?;
    let worker_b = register_worker(&app, "nas-b").await?;

    let created = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({
            "name": "  Films  ",
            "flow_id": "flow.films",
            "workers": [
                {
                    "worker_id": worker_a,
                    "local_path": "/mnt/films/",
                    "access": "read_write"
                },
                {
                    "worker_id": worker_b,
                    "local_path": "/data/films",
                    "access": "read"
                }
            ]
        })),
    )
    .await?;
    assert_eq!(created.status(), StatusCode::CREATED);
    let library = json_of(created).await?;
    assert_eq!(library["name"], "Films");
    assert_eq!(library["flow_id"], "flow.films");
    assert_eq!(library["workers"].as_array().map(Vec::len), Some(2));
    let id = library["id"].as_str().unwrap_or_default().to_owned();

    let duplicate = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({"name": "Films", "workers": []})),
    )
    .await?;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let updated = admin(
        &app,
        "PUT",
        &format!("/api/v1/libraries/{id}"),
        Some(serde_json::json!({
            "name": "Films",
            "flow_id": "",
            "workers": [{
                "worker_id": worker_a,
                "local_path": "/mnt/films",
                "access": "read"
            }]
        })),
    )
    .await?;
    assert_eq!(updated.status(), StatusCode::OK);
    let library = json_of(updated).await?;
    assert!(library["flow_id"].is_null());
    assert_eq!(library["workers"].as_array().map(Vec::len), Some(1));
    assert_eq!(library["workers"][0]["local_path"], "/mnt/films");
    assert_eq!(library["workers"][0]["access"], "read");

    let twice = admin(
        &app,
        "PUT",
        &format!("/api/v1/libraries/{id}"),
        Some(serde_json::json!({
            "name": "Films",
            "workers": [
                {
                    "worker_id": worker_a,
                    "local_path": "/mnt/a",
                    "access": "read"
                },
                {
                    "worker_id": worker_a,
                    "local_path": "/mnt/b",
                    "access": "read_write"
                }
            ]
        })),
    )
    .await?;
    assert_eq!(twice.status(), StatusCode::BAD_REQUEST);

    save_flow(&app, "flow.other", "Autre flux").await?;
    let reattached = admin(
        &app,
        "PUT",
        &format!("/api/v1/libraries/{id}"),
        Some(serde_json::json!({
            "name": "Films",
            "flow_id": "flow.other",
            "workers": [{
                "worker_id": worker_b,
                "local_path": "/data/films",
                "access": "read_write"
            }]
        })),
    )
    .await?;
    assert_eq!(reattached.status(), StatusCode::OK);

    let removed_flow = admin(&app, "DELETE", "/api/v1/flows/flow.other", None).await?;
    assert_eq!(removed_flow.status(), StatusCode::NO_CONTENT);
    let after_flow = admin(&app, "GET", &format!("/api/v1/libraries/{id}"), None).await?;
    let library = json_of(after_flow).await?;
    assert!(library["flow_id"].is_null());
    assert_eq!(library["workers"][0]["worker_id"], worker_b);

    let removed_worker =
        admin(&app, "DELETE", &format!("/api/v1/workers/{worker_b}"), None).await?;
    assert_eq!(removed_worker.status(), StatusCode::NO_CONTENT);
    let after_worker = admin(&app, "GET", &format!("/api/v1/libraries/{id}"), None).await?;
    let library = json_of(after_worker).await?;
    assert_eq!(library["workers"].as_array().map(Vec::len), Some(0));

    let removed = admin(&app, "DELETE", &format!("/api/v1/libraries/{id}"), None).await?;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let gone = admin(&app, "GET", &format!("/api/v1/libraries/{id}"), None).await?;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn upload_and_scan_library_routes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: dir.path().to_path_buf(),
        input_dir: dir.path().join("input"),
        config_dir: dir.path().join("config"),
        tools_cache_dir: dir.path().join("tools-cache"),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    config.input_dir = dir.path().join("input");
    std::fs::create_dir_all(&config.input_dir)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    let app = router(AppState::new(pool, Arc::new(config)));

    let lib = admin(
        &app,
        "POST",
        "/api/v1/libraries",
        Some(serde_json::json!({ "name": "Séries", "workers": [] })),
    )
    .await?;
    assert_eq!(lib.status(), StatusCode::CREATED);
    let lib_id = json_of(lib).await?["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing id"))?
        .to_owned();

    // 1. Upload direct dans input_dir
    let put_req = Request::builder()
        .method("PUT")
        .uri("/api/v1/files/episode1.mkv")
        .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
        .body(Body::from("video-bytes-1"))?;
    let put_resp = app.clone().oneshot(put_req).await?;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // 2. Scan de la bibliothèque -> crée 1 tâche
    let scan = admin(
        &app,
        "POST",
        &format!("/api/v1/libraries/{lib_id}/scan"),
        Some(serde_json::json!({})),
    )
    .await?;
    assert_eq!(scan.status(), StatusCode::OK);
    let scan_json = json_of(scan).await?;
    assert_eq!(scan_json["createdJobs"], 1);
    assert_eq!(scan_json["skippedExisting"], 0);

    // 3. Upload direct + création de tâche immédiate dans la bibliothèque
    let post_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/libraries/{lib_id}/files/episode2.mkv"))
        .header("authorization", format!("Bearer {}", common::ADMIN_KEY))
        .body(Body::from("video-bytes-2"))?;
    let post_resp = app.clone().oneshot(post_req).await?;
    assert_eq!(post_resp.status(), StatusCode::CREATED);
    let post_json = json_of(post_resp).await?;
    assert_eq!(post_json["file"]["name"], "episode2.mkv");
    assert_eq!(post_json["job"]["source_name"], "episode2.mkv");

    // 4. Nouveau scan -> les 2 fichiers sont déjà en file
    let rescan = admin(
        &app,
        "POST",
        &format!("/api/v1/libraries/{lib_id}/scan"),
        Some(serde_json::json!({ "dryRun": true })),
    )
    .await?;
    assert_eq!(rescan.status(), StatusCode::OK);
    let rescan_json = json_of(rescan).await?;
    assert_eq!(rescan_json["createdJobs"], 0);
    assert_eq!(rescan_json["skippedExisting"], 2);
    Ok(())
}

#[tokio::test]
async fn browse_library_filesystem_local_and_remote_worker() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let input_dir = dir.path().join("input");
    let films_dir = input_dir.join("Films");
    std::fs::create_dir_all(&films_dir)?;
    std::fs::write(films_dir.join("film.mkv"), b"mkv")?;
    std::fs::write(input_dir.join("sample.mp4"), b"mp4")?;
    std::fs::write(input_dir.join("notes.txt"), b"txt")?;

    let config = Config {
        bind: "127.0.0.1:3000".parse()?,
        database_url: "sqlite::memory:".into(),
        data_dir: dir.path().to_path_buf(),
        input_dir: input_dir.clone(),
        config_dir: dir.path().join("config"),
        tools_cache_dir: dir.path().join("tools-cache"),
        plugins_dir: None,
        static_dir: "frontend/dist".into(),
        max_file_bytes: 1024,
        local_worker: false,
        web_origin: None,
        web_session_ttl_seconds: 28_800,
        tls: None,
        smtp: None,
    };
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    common::install_keys(&pool).await?;
    let app = router(AppState::new(pool, Arc::new(config)));

    // 1. Exploration locale (sans workerId -> input_dir)
    let local_resp = admin(&app, "GET", "/api/v1/libraries/browse", None).await?;
    assert_eq!(local_resp.status(), StatusCode::OK);
    let local_json = json_of(local_resp).await?;
    assert_eq!(local_json["path"], input_dir.to_string_lossy().as_ref());
    assert_eq!(local_json["directories"].as_array().map(Vec::len), Some(1));
    assert_eq!(local_json["directories"][0]["name"], "Films");
    assert_eq!(local_json["directories"][0]["mediaCount"], 1);
    assert_eq!(local_json["files"].as_array().map(Vec::len), Some(1));
    assert_eq!(local_json["files"][0]["name"], "sample.mp4");

    // 2. Refus d'un chemin relatif / traversant
    let invalid_resp = admin(&app, "GET", "/api/v1/libraries/browse?path=../etc", None).await?;
    assert_eq!(invalid_resp.status(), StatusCode::BAD_REQUEST);

    // 3. Relais vers un worker distant en ligne
    let remote_id = register_worker(&app, "nas-browse").await?;
    let app_worker = app.clone();
    let worker_id_for_task = remote_id.clone();
    let worker_task = tokio::spawn(async move {
        let pending_req = Request::builder()
            .method("GET")
            .uri(format!(
                "/api/v1/worker/filesystem/pending?worker_id={worker_id_for_task}&wait_ms=2000"
            ))
            .header("authorization", format!("Bearer {}", common::WORKER_KEY))
            .body(Body::empty())?;
        let pending_resp = app_worker.clone().oneshot(pending_req).await?;
        assert_eq!(pending_resp.status(), StatusCode::OK);
        let tasks = json_of(pending_resp).await?;
        let task_arr = tasks
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("expected array"))?;
        assert_eq!(task_arr.len(), 1);
        let request_id = task_arr[0]["requestId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing requestId"))?;
        assert_eq!(task_arr[0]["path"], "/volume1/medias");

        let report_body = serde_json::json!({
            "requestId": request_id,
            "workerId": worker_id_for_task,
            "listing": {
                "workerId": worker_id_for_task,
                "path": "/volume1/medias",
                "parentPath": "/volume1",
                "writable": true,
                "roots": [{ "label": "Racine (/)", "path": "/", "writable": false }],
                "directories": [{
                    "name": "Séries",
                    "path": "/volume1/medias/Séries",
                    "writable": true,
                    "hasSubdirs": true,
                    "mediaCount": 12
                }],
                "files": [{
                    "name": "pilot.mkv",
                    "path": "/volume1/medias/pilot.mkv",
                    "bytes": 4096,
                    "extension": "mkv"
                }]
            },
            "error": null
        });
        let report_req = Request::builder()
            .method("POST")
            .uri("/api/v1/worker/filesystem")
            .header("authorization", format!("Bearer {}", common::WORKER_KEY))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&report_body)?))?;
        let report_resp = app_worker.oneshot(report_req).await?;
        assert_eq!(report_resp.status(), StatusCode::NO_CONTENT);
        Ok::<(), anyhow::Error>(())
    });

    let remote_resp = admin(
        &app,
        "GET",
        &format!("/api/v1/libraries/browse?workerId={remote_id}&path=%2Fvolume1%2Fmedias"),
        None,
    )
    .await?;
    assert_eq!(remote_resp.status(), StatusCode::OK);
    let remote_json = json_of(remote_resp).await?;
    assert_eq!(remote_json["path"], "/volume1/medias");
    assert_eq!(remote_json["directories"][0]["name"], "Séries");
    assert_eq!(remote_json["files"][0]["name"], "pilot.mkv");

    worker_task.await??;
    Ok(())
}
