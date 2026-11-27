//! Une session de langage : relaie les messages Forgearr de la page vers un serveur de langage, et ses
//! réponses vers la page (ADR 0032, point 4).
//!
//! La session est écrite sur des flux génériques : le lancement du processus est ailleurs. Elle prend la
//! poignée de main (`initialize`), garde le document en mémoire côté serveur (`didOpen`, `didChange`),
//! borne chaque requête à `Timings::request` et s'arrête à la fermeture par la page, après
//! `Timings::idle` sans message, ou quand le serveur se tait.

use super::{
    codec::{MAX_SERVER_MESSAGE_BYTES, read_message, write_message},
    protocol,
};
use forgearr_shared::language::{
    ClientMessage, LanguageServer, Position, ServerMessage, UnavailableReason,
};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, AsyncWriteExt},
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, sleep_until, timeout_at},
};
use uuid::Uuid;

/// Arrêt après ce délai sans message de la page.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Délai de réponse du serveur à une requête de la page.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
/// Délai de démarrage du serveur (Node.js à froid compris).
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Messages du serveur en attente d'être traités.
const FRAME_QUEUE_LEN: usize = 32;
/// Identifiant de la requête `initialize` ; les suivantes partent de 1.
const INITIALIZE_ID: i64 = 0;
const FIRST_REQUEST_ID: i64 = 1;
const SHUTDOWN_ID: i64 = i64::MAX;

/// Délais d'une session.
#[derive(Clone, Copy, Debug)]
pub struct Timings {
    pub handshake: Duration,
    pub request: Duration,
    pub idle: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            handshake: HANDSHAKE_TIMEOUT,
            request: REQUEST_TIMEOUT,
            idle: IDLE_TIMEOUT,
        }
    }
}

/// Ce qui identifie une session auprès du serveur de langage.
#[derive(Clone, Copy, Debug)]
pub struct SessionSpec {
    pub server: LanguageServer,
    pub id: Uuid,
    /// Identifiant du processus de Forgearr, annoncé au serveur (il peut s'arrêter avec lui).
    pub process_id: u32,
    pub timings: Timings,
}

/// Pourquoi une session s'est arrêtée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    /// La page a fermé la session ou le canal.
    ClientClosed,
    /// Aucun message de la page depuis `Timings::idle`.
    Idle,
    /// Le serveur ne répond plus, s'est arrêté ou n'a pas démarré.
    ServerStopped,
}

type Flow = Result<(), EndReason>;

enum PendingKind {
    Completion(u32),
    Hover(u32),
    Diagnostics,
}

struct PendingRequest {
    kind: PendingKind,
    deadline: Instant,
}

/// Lit les messages du serveur dans une tâche à part : une lecture interrompue par un `select!` perdrait
/// le message à moitié lu. Le canal se ferme quand le flux s'arrête.
fn spawn_reader<R>(mut reader: R) -> (mpsc::Receiver<Value>, JoinHandle<()>)
where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    let (frames, received) = mpsc::channel(FRAME_QUEUE_LEN);
    let task = tokio::spawn(async move {
        loop {
            match read_message(&mut reader, MAX_SERVER_MESSAGE_BYTES).await {
                Ok(Some(message)) => {
                    if frames.send(message).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) if error.is_recoverable() => {
                    tracing::debug!(%error, "message du serveur de langage ignoré");
                }
                Err(error) => {
                    tracing::debug!(%error, "flux du serveur de langage coupé");
                    break;
                }
            }
        }
    });
    (received, task)
}

struct Session<W> {
    spec: SessionSpec,
    uri: String,
    writer: W,
    to_client: mpsc::Sender<ServerMessage>,
    next_id: i64,
    pending: HashMap<i64, PendingRequest>,
    open: bool,
    /// Le serveur sait donner ses diagnostics à la demande (`ty`) : ils sont demandés à chaque changement.
    pull_supported: bool,
    pull_in_flight: bool,
    /// Le texte a changé pendant qu'une demande de diagnostics était en vol : une autre suivra.
    pull_stale: bool,
}

impl<W: AsyncWrite + Unpin> Session<W> {
    fn new(spec: SessionSpec, writer: W, to_client: mpsc::Sender<ServerMessage>) -> Self {
        Self {
            uri: protocol::document_uri(spec.id, spec.server),
            spec,
            writer,
            to_client,
            next_id: FIRST_REQUEST_ID,
            pending: HashMap::new(),
            open: false,
            pull_supported: false,
            pull_in_flight: false,
            pull_stale: false,
        }
    }

    async fn send(&mut self, message: Value) -> Flow {
        let written = write_message(&mut self.writer, &message).await;
        written.map_err(|_| EndReason::ServerStopped)
    }

    async fn emit(&mut self, message: ServerMessage) -> Flow {
        let sent = self.to_client.send(message).await;
        sent.map_err(|_| EndReason::ClientClosed)
    }

    /// Poignée de main : `initialize`, sa réponse, puis `initialized`. Les requêtes que le serveur adresse
    /// entre-temps reçoivent leur réponse. La page est prévenue par `Ready`.
    async fn handshake(&mut self, frames: &mut mpsc::Receiver<Value>) -> Flow {
        let params = protocol::initialize_params(self.spec.process_id, self.spec.id);
        self.send(protocol::request(
            INITIALIZE_ID,
            protocol::INITIALIZE,
            params,
        ))
        .await?;
        let deadline = Instant::now() + self.spec.timings.handshake;
        loop {
            let received = timeout_at(deadline, frames.recv())
                .await
                .map_err(|_| EndReason::ServerStopped)?;
            let message = received.ok_or(EndReason::ServerStopped)?;
            let answers_initialize = message.get("id") == Some(&json!(INITIALIZE_ID));
            if answers_initialize && message.get("method").is_none() {
                if message.get("error").is_some() {
                    return Err(EndReason::ServerStopped);
                }
                let provider = &message["result"]["capabilities"]["diagnosticProvider"];
                self.pull_supported = !provider.is_null();
                break;
            }
            self.on_server(message).await?;
        }
        self.send(protocol::notification(protocol::INITIALIZED, json!({})))
            .await?;
        self.emit(ServerMessage::Ready).await
    }

    async fn on_client(&mut self, message: ClientMessage) -> Flow {
        match message {
            ClientMessage::Open { text } => self.open(&text).await,
            ClientMessage::Change { version, text } => self.change(version, &text).await,
            ClientMessage::Complete { id, position } => {
                self.ask(PendingKind::Completion(id), protocol::COMPLETION, position)
                    .await
            }
            ClientMessage::Hover { id, position } => {
                self.ask(PendingKind::Hover(id), protocol::HOVER, position)
                    .await
            }
            ClientMessage::Close => Err(EndReason::ClientClosed),
        }
    }

    async fn open(&mut self, text: &str) -> Flow {
        if self.open {
            return Ok(());
        }
        let params = protocol::did_open_params(&self.uri, self.spec.server, text);
        self.send(protocol::notification(protocol::DID_OPEN, params))
            .await?;
        self.open = true;
        self.pull_diagnostics().await
    }

    async fn change(&mut self, version: u32, text: &str) -> Flow {
        if !self.open {
            return Ok(());
        }
        let params = protocol::did_change_params(&self.uri, version, text);
        self.send(protocol::notification(protocol::DID_CHANGE, params))
            .await?;
        self.pull_diagnostics().await
    }

    /// Envoie une requête de position, ou répond tout de suite à vide si le document n'est pas ouvert.
    async fn ask(&mut self, kind: PendingKind, method: &str, position: Position) -> Flow {
        if !self.open {
            return self.answer_empty(kind).await;
        }
        let params = protocol::position_params(&self.uri, position);
        self.request(kind, method, params).await
    }

    async fn request(&mut self, kind: PendingKind, method: &str, params: Value) -> Flow {
        let id = self.next_id;
        self.next_id += 1;
        let deadline = Instant::now() + self.spec.timings.request;
        self.pending.insert(id, PendingRequest { kind, deadline });
        self.send(protocol::request(id, method, params)).await
    }

    async fn pull_diagnostics(&mut self) -> Flow {
        if !self.pull_supported {
            return Ok(());
        }
        if self.pull_in_flight {
            self.pull_stale = true;
            return Ok(());
        }
        self.pull_in_flight = true;
        let params = protocol::pull_diagnostics_params(&self.uri);
        self.request(PendingKind::Diagnostics, protocol::PULL_DIAGNOSTICS, params)
            .await
    }

    async fn on_server(&mut self, message: Value) -> Flow {
        let method = message.get("method").and_then(Value::as_str);
        match (message.get("id"), method) {
            (Some(id), Some(method)) => {
                let params = message.get("params").unwrap_or(&Value::Null);
                let answer = protocol::answer_to_server_request(id, method, params);
                self.send(answer).await
            }
            (Some(id), None) => self.on_response(id.as_i64(), &message).await,
            (None, Some(method)) => self.on_notification(method, &message).await,
            (None, None) => Ok(()),
        }
    }

    async fn on_response(&mut self, id: Option<i64>, message: &Value) -> Flow {
        let Some(request) = id.and_then(|id| self.pending.remove(&id)) else {
            return Ok(());
        };
        let result = message.get("result").unwrap_or(&Value::Null);
        match request.kind {
            PendingKind::Completion(id) => {
                let items = protocol::suggestions_from(result);
                self.emit(ServerMessage::Completion { id, items }).await
            }
            PendingKind::Hover(id) => {
                let text = protocol::hover_text_from(result);
                self.emit(ServerMessage::Hover { id, text }).await
            }
            PendingKind::Diagnostics => self.on_pulled_diagnostics(result).await,
        }
    }

    async fn on_pulled_diagnostics(&mut self, result: &Value) -> Flow {
        self.pull_in_flight = false;
        let unchanged = result.get("kind").and_then(Value::as_str) == Some("unchanged");
        if !result.is_null() && !unchanged {
            let items = result.get("items").unwrap_or(&Value::Null);
            self.emit_diagnostics(items).await?;
        }
        if self.pull_stale {
            self.pull_stale = false;
            self.pull_diagnostics().await?;
        }
        Ok(())
    }

    async fn on_notification(&mut self, method: &str, message: &Value) -> Flow {
        if method != protocol::PUBLISH_DIAGNOSTICS {
            return Ok(());
        }
        let params = message.get("params").unwrap_or(&Value::Null);
        let ours = params.get("uri").and_then(Value::as_str) == Some(self.uri.as_str());
        if !ours {
            return Ok(());
        }
        let items = params.get("diagnostics").unwrap_or(&Value::Null);
        self.emit_diagnostics(items).await
    }

    async fn emit_diagnostics(&mut self, items: &Value) -> Flow {
        let items = protocol::diagnostics_from(items, self.spec.server);
        self.emit(ServerMessage::Diagnostics { items }).await
    }

    /// Réponse vide à une requête restée sans réponse : la page garde sa liste locale.
    async fn answer_empty(&mut self, kind: PendingKind) -> Flow {
        match kind {
            PendingKind::Completion(id) => {
                let items = Vec::new();
                self.emit(ServerMessage::Completion { id, items }).await
            }
            PendingKind::Hover(id) => self.emit(ServerMessage::Hover { id, text: None }).await,
            PendingKind::Diagnostics => {
                self.pull_in_flight = false;
                Ok(())
            }
        }
    }

    /// Répond à vide aux requêtes dont le délai est passé.
    async fn expire(&mut self) -> Flow {
        let now = Instant::now();
        for id in due_ids(&self.pending, now) {
            if let Some(request) = self.pending.remove(&id) {
                self.answer_empty(request.kind).await?;
            }
        }
        Ok(())
    }

    /// Demande son arrêt au serveur (`shutdown`, `exit`, fermeture de stdin) : au mieux, le processus est
    /// de toute façon tué à la fin.
    async fn shutdown(&mut self) {
        let _ = self
            .send(protocol::request(
                SHUTDOWN_ID,
                protocol::SHUTDOWN,
                Value::Null,
            ))
            .await;
        let _ = self
            .send(protocol::notification(protocol::EXIT, Value::Null))
            .await;
        let _ = self.writer.shutdown().await;
    }
}

/// Les requêtes dont le délai est passé.
fn due_ids(pending: &HashMap<i64, PendingRequest>, now: Instant) -> Vec<i64> {
    pending
        .iter()
        .filter(|(_, request)| request.deadline <= now)
        .map(|(id, _)| *id)
        .collect()
}

/// Le plus proche délai de réponse en cours.
fn next_deadline(pending: &HashMap<i64, PendingRequest>) -> Option<Instant> {
    pending.values().map(|request| request.deadline).min()
}

async fn wait_for(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

async fn drive<W: AsyncWrite + Unpin>(
    session: &mut Session<W>,
    frames: &mut mpsc::Receiver<Value>,
    from_client: &mut mpsc::Receiver<ClientMessage>,
) -> EndReason {
    if let Err(reason) = session.handshake(frames).await {
        return reason;
    }
    let idle = session.spec.timings.idle;
    let mut idle_deadline = Instant::now() + idle;
    loop {
        let next_request_deadline = next_deadline(&session.pending);
        let outcome = tokio::select! {
            message = from_client.recv() => match message {
                Some(message) => {
                    idle_deadline = Instant::now() + idle;
                    session.on_client(message).await
                }
                None => Err(EndReason::ClientClosed),
            },
            frame = frames.recv() => match frame {
                Some(message) => session.on_server(message).await,
                None => Err(EndReason::ServerStopped),
            },
            () = wait_for(next_request_deadline) => session.expire().await,
            () = sleep_until(idle_deadline) => Err(EndReason::Idle),
        };
        if let Err(reason) = outcome {
            return reason;
        }
    }
}

/// Mène une session jusqu'à son arrêt. `reader` et `writer` sont la sortie et l'entrée du serveur de
/// langage. À l'arrêt, la page est prévenue (`Unavailable`) sauf si c'est elle qui a fermé.
pub async fn run<R, W>(
    spec: SessionSpec,
    reader: R,
    writer: W,
    mut from_client: mpsc::Receiver<ClientMessage>,
    to_client: mpsc::Sender<ServerMessage>,
) -> EndReason
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
{
    let (mut frames, reader_task) = spawn_reader(reader);
    let mut session = Session::new(spec, writer, to_client.clone());
    let reason = drive(&mut session, &mut frames, &mut from_client).await;
    session.shutdown().await;
    reader_task.abort();
    if reason != EndReason::ClientClosed {
        let reason = UnavailableReason::ServerStopped;
        let _ = to_client.send(ServerMessage::Unavailable { reason }).await;
    }
    reason
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
