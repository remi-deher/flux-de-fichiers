//! Canal des suggestions de code de l'éditeur de script : `/api/v1/language/session` (ADR 0032, lot b1).

use super::{
    AppState, ManagePluginsAuth, auth::authenticate_request, events::ACCESS_CHECK_INTERVAL,
};
use crate::{
    error::{AppError, AppResult},
    language::{
        launch::{self, LaunchError, LaunchedServer},
        relay::{self, refuse},
        session::{SessionSpec, Timings},
    },
    web_session::validate_origin,
};
use axum::{
    extract::{
        Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Method},
    response::Response,
};
use forgearr_shared::{
    Permission,
    language::{LANGUAGE_TOOL_ID, LanguageServer, MAX_CLIENT_MESSAGE_BYTES, UnavailableReason},
    local::ScriptInterpreter,
};
use serde::Deserialize;
use uuid::Uuid;

/// Paramètres d'ouverture : l'interpréteur du script édité (`interpreter`).
#[derive(Debug, Deserialize)]
pub struct SessionQuery {
    interpreter: ScriptInterpreter,
}

/// Ouvre une session de langage sur un WebSocket. Le navigateur ne pouvant pas poser d'en-tête
/// d'authentification, l'appelant s'identifie par son cookie de session, et son origine est contrôlée.
pub async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
    _auth: ManagePluginsAuth,
    Query(query): Query<SessionQuery>,
    upgrade: WebSocketUpgrade,
) -> AppResult<Response> {
    validate_origin(&headers, &state.config)?;
    Ok(upgrade
        .max_message_size(MAX_CLIENT_MESSAGE_BYTES)
        .on_upgrade(move |socket| serve(state, headers, query.interpreter, socket)))
}

async fn serve(
    state: AppState,
    headers: HeaderMap,
    interpreter: ScriptInterpreter,
    socket: WebSocket,
) {
    let Some(server) = LanguageServer::for_interpreter(interpreter) else {
        return refuse(socket, UnavailableReason::InterpreterUnsupported).await;
    };
    let Some(_place) = state.language_sessions.try_acquire() else {
        return refuse(socket, UnavailableReason::LimitReached).await;
    };
    // Aucun bail ne dit à la purge que le serveur lit les outils : elle attend la fin de la session.
    let _tools = state.tools_in_use.clone().read_owned().await;
    let id = Uuid::new_v4();
    let launched = match start(&state, server, id).await {
        Ok(launched) => launched,
        Err(error) => {
            tracing::warn!(%error, ?interpreter, "serveur de langage indisponible");
            return refuse(socket, UnavailableReason::ServerMissing).await;
        }
    };
    let LaunchedServer {
        reader,
        writer,
        child,
        workspace,
    } = launched;
    let spec = SessionSpec {
        server,
        id,
        process_id: std::process::id(),
        timings: Timings::default(),
    };
    relay::relay(socket, spec, reader, writer, ACCESS_CHECK_INTERVAL, || {
        still_allowed(&state, &headers)
    })
    .await;
    drop(child);
    drop(workspace);
}

/// Lance le serveur depuis la version installée de l'application `langages`.
async fn start(
    state: &AppState,
    server: LanguageServer,
    session: Uuid,
) -> Result<LaunchedServer, LaunchError> {
    let engine = crate::plugins::engine_sources(&state.pool, &state.config)
        .await
        .map_err(|error| LaunchError::Failed(format!("{error:?}")))?;
    let version = state
        .catalog_cache
        .resolve_tool_version(&engine.roots, LANGUAGE_TOOL_ID)
        .await;
    let tool_dir = launch::installed_tool_dir(&state.tool_cache(), &version)
        .ok_or(LaunchError::Missing(LANGUAGE_TOOL_ID))?;
    let launcher =
        std::env::current_exe().map_err(|error| LaunchError::Failed(error.to_string()))?;
    launch::launch(
        &launcher,
        &tool_dir,
        &state.config.data_dir,
        server,
        session,
    )
}

/// L'appelant a-t-il encore le droit de gérer les applications ? Une base momentanément indisponible ne
/// coupe pas le canal.
async fn still_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    match authenticate_request(state, headers, &Method::GET).await {
        Ok(principal) => principal.allows(Permission::ManagePlugins),
        Err(AppError::Unauthorized | AppError::Forbidden) => false,
        Err(error) => {
            tracing::warn!(?error, "accès au canal de langage non revérifié");
            true
        }
    }
}

#[cfg(test)]
#[path = "language_tests.rs"]
mod tests;
