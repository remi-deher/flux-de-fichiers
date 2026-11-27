//! Relais entre le WebSocket de la page et une session de langage (ADR 0032, point 3).
//!
//! La page n'envoie que des messages Forgearr typés : un texte illisible, un message binaire ou un message
//! trop gros ferment le canal. Les droits de l'appelant sont revérifiés à intervalle régulier.

use super::session::{self, SessionSpec};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use forgearr_shared::language::{ClientMessage, ServerMessage, UnavailableReason};
use std::{future::Future, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncWrite},
    sync::mpsc,
    time::{Instant, MissedTickBehavior, interval_at, timeout},
};

/// Messages en attente de chaque côté du relais.
const QUEUE_LEN: usize = 32;
/// Attente de la fin de la session une fois le canal fermé : elle arrête alors son serveur.
const SESSION_END_TIMEOUT: Duration = Duration::from_secs(5);
/// Code de fermeture d'un message que la page n'a pas le droit d'envoyer.
const CLOSE_POLICY_VIOLATION: u16 = 1008;
/// Code de fermeture d'un canal dont l'accès est retiré.
const CLOSE_NORMAL: u16 = 1000;
const REASON_INVALID_MESSAGE: &str = "message non autorisé";
const REASON_ACCESS_WITHDRAWN: &str = "accès retiré";

fn text_of(message: &ServerMessage) -> Message {
    Message::text(serde_json::to_string(message).unwrap_or_default())
}

fn close_with(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }))
}

/// Dit à la page que la session n'a pas de serveur de langage, puis ferme le canal.
pub async fn refuse(mut socket: WebSocket, reason: UnavailableReason) {
    let message = ServerMessage::Unavailable { reason };
    if socket.send(text_of(&message)).await.is_ok() {
        let _ = socket.send(close_with(CLOSE_NORMAL, "")).await;
    }
}

/// Ce que la page a envoyé.
#[derive(Debug, PartialEq, Eq)]
enum Incoming {
    Message(ClientMessage),
    Ignored,
    Closed,
    Refused,
}

fn incoming(frame: Option<Result<Message, axum::Error>>) -> Incoming {
    match frame {
        Some(Ok(Message::Text(text))) => {
            serde_json::from_str(text.as_str()).map_or(Incoming::Refused, Incoming::Message)
        }
        Some(Ok(Message::Binary(_))) => Incoming::Refused,
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Incoming::Ignored,
        Some(Ok(Message::Close(_)) | Err(_)) | None => Incoming::Closed,
    }
}

/// Mène la session jusqu'à sa fin, côté page comme côté serveur. `allowed` est interrogé toutes les
/// `check_every` : le canal est fermé dès qu'il répond `false`.
pub async fn relay<R, W, F, Fut>(
    mut socket: WebSocket,
    spec: SessionSpec,
    reader: R,
    writer: W,
    check_every: Duration,
    mut allowed: F,
) where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let (to_session, from_client) = mpsc::channel(QUEUE_LEN);
    let (to_client, mut from_session) = mpsc::channel(QUEUE_LEN);
    let mut session = tokio::spawn(session::run(spec, reader, writer, from_client, to_client));
    let mut check = interval_at(Instant::now() + check_every, check_every);
    check.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut farewell = None;
    loop {
        tokio::select! {
            frame = socket.recv() => match incoming(frame) {
                Incoming::Message(message) => {
                    if to_session.send(message).await.is_err() {
                        break;
                    }
                }
                Incoming::Ignored => {}
                Incoming::Closed => break,
                Incoming::Refused => {
                    farewell = Some(close_with(CLOSE_POLICY_VIOLATION, REASON_INVALID_MESSAGE));
                    break;
                }
            },
            outgoing = from_session.recv() => {
                let Some(message) = outgoing else { break };
                if socket.send(text_of(&message)).await.is_err() {
                    break;
                }
            }
            _ = check.tick() => {
                if !allowed().await {
                    farewell = Some(close_with(CLOSE_POLICY_VIOLATION, REASON_ACCESS_WITHDRAWN));
                    break;
                }
            }
        }
    }
    drop(to_session);
    if timeout(SESSION_END_TIMEOUT, &mut session).await.is_err() {
        session.abort();
    }
    while let Ok(message) = from_session.try_recv() {
        if socket.send(text_of(&message)).await.is_err() {
            return;
        }
    }
    let _ = socket
        .send(farewell.unwrap_or_else(|| close_with(CLOSE_NORMAL, "")))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::language::Position;

    fn text(raw: &str) -> Option<Result<Message, axum::Error>> {
        Some(Ok(Message::text(raw)))
    }

    #[test]
    fn a_typed_message_is_passed_on() {
        let parsed = incoming(text(r#"{"type":"close"}"#));
        assert_eq!(parsed, Incoming::Message(ClientMessage::Close));
        let parsed = incoming(text(
            r#"{"type":"hover","id":3,"position":{"line":1,"character":2}}"#,
        ));
        let position = Position {
            line: 1,
            character: 2,
        };
        assert_eq!(
            parsed,
            Incoming::Message(ClientMessage::Hover { id: 3, position })
        );
    }

    #[test]
    fn anything_else_from_the_page_is_refused() {
        assert_eq!(incoming(text("pas du json")), Incoming::Refused);
        let method = text(r#"{"method":"initialize"}"#);
        assert_eq!(incoming(method), Incoming::Refused);
        let binary = Some(Ok(Message::Binary(vec![1, 2].into())));
        assert_eq!(incoming(binary), Incoming::Refused);
    }

    #[test]
    fn a_closed_channel_and_control_frames_are_told_apart() {
        assert_eq!(incoming(None), Incoming::Closed);
        let close = Some(Ok(Message::Close(None)));
        assert_eq!(incoming(close), Incoming::Closed);
        let ping = Some(Ok(Message::Ping(Vec::new().into())));
        assert_eq!(incoming(ping), Incoming::Ignored);
    }

    #[test]
    fn a_server_message_is_sent_as_json_text() {
        let message = text_of(&ServerMessage::Ready);
        assert_eq!(message, Message::text(r#"{"type":"ready"}"#));
    }
}
