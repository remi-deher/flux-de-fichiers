//! Canal des suggestions de code sur un vrai WebSocket (ADR 0032, lot b1) : accès, refus annoncés à la page,
//! relais d'une session et fermeture des messages interdits.

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{Router, extract::ws::WebSocketUpgrade, response::Response, routing::get};
use forgearr::{
    api::router,
    language::{
        codec::{read_message, write_message},
        limit::SessionLimiter,
        relay::relay,
        session::{SessionSpec, Timings},
    },
};
use forgearr_shared::language::{LANGUAGE_SESSION_PATH, LanguageServer};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{BufReader, DuplexStream, duplex},
    net::TcpStream,
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{
        Error as WebSocketError, Message, client::IntoClientRequest, http::HeaderValue,
    },
};
use uuid::Uuid;

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(10);
const PIPE_BYTES: usize = 64 * 1024;
const MAX_FAKE_MESSAGE_BYTES: usize = 1 << 20;
const RELAY_PATH: &str = "/relay";
const REVOKED_PATH: &str = "/relay-revoked";
const CHECK_EVERY: Duration = Duration::from_millis(500);
const FAR_CHECK: Duration = Duration::from_secs(3600);
const POLICY_VIOLATION_CODE: u16 = 1008;
/// Code LSP de `CompletionItemKind::Function`.
const COMPLETION_FUNCTION_KIND: u8 = 3;

async fn serve(app: Router) -> anyhow::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let service = app.into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, service).await;
    });
    Ok(address)
}

fn header_value(value: &str) -> Result<HeaderValue, WebSocketError> {
    HeaderValue::from_str(value).map_err(|error| WebSocketError::HttpFormat(error.into()))
}

async fn connect(
    address: SocketAddr,
    path: &str,
    origin: Option<&str>,
    key: Option<&str>,
) -> Result<Client, WebSocketError> {
    let mut request = format!("ws://{address}{path}").into_client_request()?;
    let headers = request.headers_mut();
    if let Some(origin) = origin {
        headers.insert("origin", header_value(origin)?);
    }
    if let Some(key) = key {
        headers.insert("authorization", header_value(&format!("Bearer {key}"))?);
    }
    let (client, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(client)
}

fn refused_with(result: Result<Client, WebSocketError>) -> Option<u16> {
    match result {
        Err(WebSocketError::Http(response)) => Some(response.status().as_u16()),
        _ => None,
    }
}

async fn next_frame(client: &mut Client) -> anyhow::Result<Message> {
    let frame = timeout(WAIT, client.next()).await?;
    Ok(frame.ok_or_else(|| anyhow::anyhow!("canal fermé"))??)
}

async fn next_json(client: &mut Client) -> anyhow::Result<Value> {
    match next_frame(client).await? {
        Message::Text(text) => Ok(serde_json::from_str(text.as_str())?),
        other => anyhow::bail!("message inattendu : {other:?}"),
    }
}

async fn send_json(client: &mut Client, message: Value) -> anyhow::Result<()> {
    client
        .send(Message::Text(message.to_string().into()))
        .await?;
    Ok(())
}

/// Code de la fermeture que le central annonce.
async fn close_code(client: &mut Client) -> anyhow::Result<Option<u16>> {
    match next_frame(client).await? {
        Message::Close(frame) => Ok(frame.map(|frame| u16::from(frame.code))),
        other => anyhow::bail!("fermeture attendue, reçu {other:?}"),
    }
}

struct CentralFixture {
    address: SocketAddr,
    _dir: tempfile::TempDir,
}

async fn central(limiter: Option<SessionLimiter>) -> anyhow::Result<CentralFixture> {
    let dir = tempfile::tempdir()?;
    let mut state = installation::blank_state(dir.path()).await?;
    common::install_keys(&state.pool).await?;
    common::mark_store_migrated(&state.pool).await?;
    if let Some(limiter) = limiter {
        state.language_sessions = limiter;
    }
    let address = serve(router(state)).await?;
    Ok(CentralFixture { address, _dir: dir })
}

fn session_path(interpreter: &str) -> String {
    format!("{LANGUAGE_SESSION_PATH}?interpreter={interpreter}")
}

fn own_origin(address: SocketAddr) -> String {
    format!("http://{address}")
}

#[tokio::test]
async fn a_caller_without_credentials_is_refused() -> anyhow::Result<()> {
    let central = central(None).await?;
    let origin = own_origin(central.address);
    let path = session_path("python3");
    let result = connect(central.address, &path, Some(&origin), None).await;
    assert_eq!(refused_with(result), Some(401));
    Ok(())
}

#[tokio::test]
async fn a_caller_without_the_right_is_refused() -> anyhow::Result<()> {
    let central = central(None).await?;
    let origin = own_origin(central.address);
    let path = session_path("python3");
    let key = Some(common::WORKER_KEY);
    let result = connect(central.address, &path, Some(&origin), key).await;
    assert_eq!(refused_with(result), Some(403));
    Ok(())
}

#[tokio::test]
async fn a_foreign_origin_is_refused() -> anyhow::Result<()> {
    let central = central(None).await?;
    let path = session_path("python3");
    let key = Some(common::ADMIN_KEY);
    let foreign = connect(central.address, &path, Some("http://evil.example"), key).await;
    assert_eq!(refused_with(foreign), Some(403));
    let none = connect(central.address, &path, None, key).await;
    assert_eq!(refused_with(none), Some(403));
    Ok(())
}

#[tokio::test]
async fn an_unknown_interpreter_is_refused() -> anyhow::Result<()> {
    let central = central(None).await?;
    let origin = own_origin(central.address);
    let path = session_path("cobol");
    let key = Some(common::ADMIN_KEY);
    let result = connect(central.address, &path, Some(&origin), key).await;
    assert_eq!(refused_with(result), Some(400));
    Ok(())
}

async fn unavailable_reason(central: &CentralFixture, interpreter: &str) -> anyhow::Result<Value> {
    let origin = own_origin(central.address);
    let path = session_path(interpreter);
    let key = Some(common::ADMIN_KEY);
    let mut client = connect(central.address, &path, Some(&origin), key).await?;
    let message = next_json(&mut client).await?;
    assert_eq!(message["type"], "unavailable");
    assert_eq!(close_code(&mut client).await?, Some(1000));
    Ok(message["reason"].clone())
}

#[tokio::test]
async fn an_interpreter_without_a_server_stays_on_the_local_list() -> anyhow::Result<()> {
    let central = central(None).await?;
    let reason = unavailable_reason(&central, "node").await?;
    assert_eq!(reason, "interpreter-unsupported");
    Ok(())
}

#[tokio::test]
async fn a_missing_application_is_announced_to_the_page() -> anyhow::Result<()> {
    let central = central(None).await?;
    let reason = unavailable_reason(&central, "python3").await?;
    assert_eq!(reason, "server-missing");
    Ok(())
}

#[tokio::test]
async fn the_session_cap_is_announced_to_the_page() -> anyhow::Result<()> {
    let central = central(Some(SessionLimiter::new(0))).await?;
    let reason = unavailable_reason(&central, "bash").await?;
    assert_eq!(reason, "limit-reached");
    Ok(())
}

/// Serveur de langage joué par le test : il répond à chaque requête, sans rien publier de lui-même.
async fn fake_language_server(from_session: DuplexStream, mut to_session: DuplexStream) {
    let mut reads = BufReader::new(from_session);
    while let Ok(Some(message)) = read_message(&mut reads, MAX_FAKE_MESSAGE_BYTES).await {
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let result = match message["method"].as_str() {
            Some("textDocument/completion") => {
                json!([{"label": "dirname", "kind": COMPLETION_FUNCTION_KIND}])
            }
            _ => json!({"capabilities": {}}),
        };
        let answer = json!({"jsonrpc": "2.0", "id": id, "result": result});
        if write_message(&mut to_session, &answer).await.is_err() {
            return;
        }
    }
}

async fn relayed(upgrade: WebSocketUpgrade, check_every: Duration, allowed: bool) -> Response {
    upgrade.on_upgrade(move |socket| async move {
        let (session_writer, fake_reads) = duplex(PIPE_BYTES);
        let (fake_writes, session_reader) = duplex(PIPE_BYTES);
        tokio::spawn(fake_language_server(fake_reads, fake_writes));
        let spec = SessionSpec {
            server: LanguageServer::Bash,
            id: Uuid::nil(),
            process_id: std::process::id(),
            timings: Timings::default(),
        };
        let reader = BufReader::new(session_reader);
        let still_allowed = || async move { allowed };
        relay(socket, spec, reader, session_writer, check_every, still_allowed).await;
    })
}

async fn relay_server() -> anyhow::Result<SocketAddr> {
    let app = Router::new()
        .route(
            RELAY_PATH,
            get(|upgrade: WebSocketUpgrade| relayed(upgrade, FAR_CHECK, true)),
        )
        .route(
            REVOKED_PATH,
            get(|upgrade: WebSocketUpgrade| relayed(upgrade, CHECK_EVERY, false)),
        );
    serve(app).await
}

async fn ready_client(address: SocketAddr, path: &str) -> anyhow::Result<Client> {
    let mut client = connect(address, path, None, None).await?;
    assert_eq!(next_json(&mut client).await?, json!({"type": "ready"}));
    Ok(client)
}

#[tokio::test]
async fn a_page_completes_through_the_relay() -> anyhow::Result<()> {
    let address = relay_server().await?;
    let mut client = ready_client(address, RELAY_PATH).await?;
    send_json(&mut client, json!({"type": "open", "text": "dir"})).await?;
    let position = json!({"line": 0, "character": 3});
    let complete = json!({"type": "complete", "id": 7, "position": position});
    send_json(&mut client, complete).await?;
    let answer = next_json(&mut client).await?;
    assert_eq!(answer["type"], "completion");
    assert_eq!(answer["id"], 7);
    assert_eq!(answer["items"][0]["label"], "dirname");
    assert_eq!(answer["items"][0]["kind"], "function");
    Ok(())
}

#[tokio::test]
async fn a_message_the_page_may_not_send_closes_the_channel() -> anyhow::Result<()> {
    let address = relay_server().await?;
    for raw in [
        Message::Text(r#"{"method":"workspace/executeCommand"}"#.into()),
        Message::Text("pas du json".into()),
        Message::Binary(vec![1, 2, 3].into()),
    ] {
        let mut client = ready_client(address, RELAY_PATH).await?;
        client.send(raw).await?;
        assert_eq!(close_code(&mut client).await?, Some(POLICY_VIOLATION_CODE));
    }
    Ok(())
}

#[tokio::test]
async fn an_oversized_message_closes_the_channel() -> anyhow::Result<()> {
    let address = relay_server().await?;
    let mut client = ready_client(address, RELAY_PATH).await?;
    let text = "x".repeat(forgearr_shared::language::MAX_CLIENT_MESSAGE_BYTES + 1);
    send_json(&mut client, json!({"type": "open", "text": text})).await?;
    let outcome = timeout(WAIT, async {
        while let Some(frame) = client.next().await {
            if !matches!(frame, Ok(Message::Text(_))) {
                return;
            }
        }
    });
    outcome.await?;
    Ok(())
}

#[tokio::test]
async fn a_caller_who_loses_the_right_loses_the_channel() -> anyhow::Result<()> {
    let address = relay_server().await?;
    let mut client = ready_client(address, REVOKED_PATH).await?;
    assert_eq!(close_code(&mut client).await?, Some(POLICY_VIOLATION_CODE));
    Ok(())
}

#[tokio::test]
async fn closing_the_page_ends_the_session() -> anyhow::Result<()> {
    let address = relay_server().await?;
    let mut client = ready_client(address, RELAY_PATH).await?;
    send_json(&mut client, json!({"type": "close"})).await?;
    assert_eq!(close_code(&mut client).await?, Some(1000));
    Ok(())
}
