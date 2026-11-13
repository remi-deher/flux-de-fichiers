//! Port HTTP d'un worker : santé et état du nœud, mêmes routes publiques que la centrale (ADR 0011).

use crate::config;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use flux_shared::NodeKind;
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Ce que le worker sait de lui-même ; la boucle d'agent met les indicateurs à jour.
pub struct WorkerState {
    pub name: String,
    pub coordinator_url: String,
    registered: AtomicBool,
    coordinator_reachable: AtomicBool,
}

impl WorkerState {
    pub fn new(name: &str, coordinator_url: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_owned(),
            coordinator_url: coordinator_url.to_owned(),
            registered: AtomicBool::new(false),
            coordinator_reachable: AtomicBool::new(false),
        })
    }

    pub fn mark_registered(&self) {
        self.registered.store(true, Ordering::Relaxed);
    }

    pub fn mark_coordinator_reachable(&self, reachable: bool) {
        self.coordinator_reachable
            .store(reachable, Ordering::Relaxed);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerStatus {
    node_kind: NodeKind,
    name: String,
    coordinator_url: String,
    version: &'static str,
    registered: bool,
    coordinator_reachable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupStatus {
    needs_setup: bool,
    instance_name: String,
    node_kind: NodeKind,
}

async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn status(State(state): State<Arc<WorkerState>>) -> Json<WorkerStatus> {
    Json(WorkerStatus {
        node_kind: NodeKind::Worker,
        name: state.name.clone(),
        coordinator_url: state.coordinator_url.clone(),
        version: env!("CARGO_PKG_VERSION"),
        registered: state.registered.load(Ordering::Relaxed),
        coordinator_reachable: state.coordinator_reachable.load(Ordering::Relaxed),
    })
}

/// Même route que la centrale : l'interface sait ainsi qu'elle parle à un worker déjà enrôlé.
async fn setup_status(State(state): State<Arc<WorkerState>>) -> Json<SetupStatus> {
    Json(SetupStatus {
        needs_setup: false,
        instance_name: state.name.clone(),
        node_kind: NodeKind::Worker,
    })
}

pub fn router(state: Arc<WorkerState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/node/status", get(status))
        .route("/api/v1/setup/status", get(setup_status))
        .with_state(state)
}

/// Sert le port HTTP du worker jusqu'à l'arrêt. Un port occupé n'empêche pas le worker de travailler.
pub async fn serve(state: Arc<WorkerState>) {
    let bind = match config::bind_address() {
        Ok(bind) => bind,
        Err(error) => {
            tracing::warn!(error = %error, "adresse d'écoute du worker invalide");
            return;
        }
    };
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!(error = %error, %bind, "port HTTP du worker indisponible");
            return;
        }
    };
    tracing::info!(%bind, "worker HTTP started");
    let served = axum::serve(listener, router(state))
        .with_graceful_shutdown(crate::api::shutdown_signal())
        .await;
    if let Err(error) = served {
        tracing::warn!(error = %error, "serveur HTTP du worker arrêté");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn get_json(app: Router, path: &str) -> anyhow::Result<(StatusCode, serde_json::Value)> {
        let response = app
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        Ok((status, body))
    }

    #[tokio::test]
    async fn test_health_is_always_up_while_the_process_runs() -> anyhow::Result<()> {
        let app = router(WorkerState::new("nas-1", "http://central:3000"));
        let (status, _) = get_json(app, "/health").await?;
        assert_eq!(status, StatusCode::NO_CONTENT);
        Ok(())
    }

    #[tokio::test]
    async fn test_status_follows_the_agent_loop() -> anyhow::Result<()> {
        let state = WorkerState::new("nas-1", "http://central:3000");
        let (_, before) = get_json(router(state.clone()), "/api/v1/node/status").await?;
        assert_eq!(before["nodeKind"], "worker");
        assert_eq!(before["name"], "nas-1");
        assert_eq!(before["registered"], false);
        assert_eq!(before["coordinatorReachable"], false);
        state.mark_registered();
        state.mark_coordinator_reachable(true);
        let (_, after) = get_json(router(state), "/api/v1/node/status").await?;
        assert_eq!(after["registered"], true);
        assert_eq!(after["coordinatorReachable"], true);
        Ok(())
    }

    #[tokio::test]
    async fn test_setup_status_never_offers_the_wizard_on_a_worker() -> anyhow::Result<()> {
        let app = router(WorkerState::new("nas-1", "http://central:3000"));
        let (status, body) = get_json(app, "/api/v1/setup/status").await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["needsSetup"], false);
        assert_eq!(body["nodeKind"], "worker");
        assert_eq!(body["instanceName"], "nas-1");
        Ok(())
    }
}
