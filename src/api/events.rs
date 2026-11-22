//! Flux SSE des événements : `/api/v1/events` (tout) et `/api/v1/jobs/events` (tâches seulement).

use super::{AnyAuth, AppState, ViewJobsAuth, auth::authenticate_request};
use crate::{
    auth::Principal,
    error::{AppError, AppResult},
    live::{LiveEvent, Subscription},
};
use axum::{
    extract::State,
    http::{HeaderMap, Method},
    response::sse::{Event, KeepAlive, Sse},
};
use forgearr_shared::live::{
    CHANGED_EVENT, ChangeTopic, ChangedEvent, JOB_EVENTS_PERMISSION, RESYNC_EVENT,
};
use futures_util::stream::Stream;
use std::{convert::Infallible, time::Duration};
use tokio::{
    sync::broadcast::error::RecvError,
    time::{Instant, Interval, MissedTickBehavior},
};

/// Intervalle des messages de maintien du flux d'événements, pour traverser les proxys.
const SSE_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);
/// Délai au bout duquel une session expirée, révoquée ou privée de ses droits perd son flux.
const ACCESS_CHECK_INTERVAL: Duration = Duration::from_secs(30);
/// Un événement SSE sans donnée n'est jamais remis au navigateur.
const EMPTY_DATA: &str = "{}";

/// Ce que transporte un flux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Jobs,
    All,
}

impl Scope {
    fn admits(self, principal: &Principal) -> bool {
        principal.allows(JOB_EVENTS_PERMISSION)
            || (self == Self::All
                && ChangeTopic::ALL
                    .into_iter()
                    .any(|topic| can_view(principal, topic)))
    }

    fn delivers(self, principal: &Principal, event: &LiveEvent) -> bool {
        match event {
            LiveEvent::Job(_) => principal.allows(JOB_EVENTS_PERMISSION),
            LiveEvent::Changed(topic) => self == Self::All && can_view(principal, *topic),
        }
    }
}

fn can_view(principal: &Principal, topic: ChangeTopic) -> bool {
    topic
        .viewers()
        .iter()
        .any(|permission| principal.allows(*permission))
}

struct Cursor {
    state: AppState,
    headers: HeaderMap,
    principal: Principal,
    scope: Scope,
    /// Annonce de reprise, envoyée avant tout événement.
    opening: Option<Event>,
    subscription: Subscription,
    access_check: Interval,
}

impl Cursor {
    async fn next(mut self) -> Option<(Result<Event, Infallible>, Self)> {
        if let Some(event) = self.opening.take() {
            return Some((Ok(event), self));
        }
        loop {
            let (seq, event) = if let Some(missed) = self.subscription.replay.pop_front() {
                missed
            } else {
                let received = tokio::select! {
                    received = self.subscription.rx.recv() => Some(received),
                    _ = self.access_check.tick() => None,
                };
                match received {
                    Some(Ok(event)) => event,
                    Some(Err(RecvError::Lagged(_))) => return Some((Ok(resync_event()), self)),
                    Some(Err(RecvError::Closed)) => return None,
                    None => {
                        if !self.still_admitted().await {
                            return None;
                        }
                        continue;
                    }
                }
            };
            if self.scope.delivers(&self.principal, &event) {
                return Some((Ok(format_event(seq, &event)), self));
            }
        }
    }

    /// L'appelant a-t-il encore accès au flux ? Une base momentanément indisponible ne le coupe pas.
    async fn still_admitted(&mut self) -> bool {
        match authenticate_request(&self.state, &self.headers, &Method::GET).await {
            Ok(principal) => {
                self.principal = principal;
                self.scope.admits(&self.principal)
            }
            Err(AppError::Unauthorized | AppError::Forbidden) => false,
            Err(error) => {
                tracing::warn!(?error, "accès au flux d'événements non revérifié");
                true
            }
        }
    }
}

fn format_event(seq: u64, event: &LiveEvent) -> Event {
    let (name, data) = match event {
        LiveEvent::Job(job) => (job.event_type.as_str(), serde_json::to_string(job)),
        LiveEvent::Changed(topic) => (
            CHANGED_EVENT,
            serde_json::to_string(&ChangedEvent { topic: *topic }),
        ),
    };
    Event::default()
        .id(seq.to_string())
        .event(name)
        .data(data.unwrap_or_default())
}

fn resync_event() -> Event {
    Event::default().event(RESYNC_EVENT).data(EMPTY_DATA)
}

/// Donne au client sa position : sans donnée, l'événement n'est pas remis mais fixe le
/// `Last-Event-ID` d'une reconnexion survenue avant tout événement.
fn opening_event(subscription: &Subscription) -> Event {
    let event = if subscription.resync {
        resync_event()
    } else {
        Event::default()
    };
    event.id(subscription.position.to_string())
}

async fn open(
    state: AppState,
    headers: HeaderMap,
    principal: Principal,
    scope: Scope,
    check_every: Duration,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    if !scope.admits(&principal) {
        return Err(AppError::Forbidden);
    }
    // Un identifiant illisible vient d'ailleurs : il est traité comme celui d'une autre instance.
    let last_event_id = headers.get("last-event-id").map(|value| {
        value
            .to_str()
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .unwrap_or_default()
    });
    let subscription = state.event_history.subscribe_from(last_event_id).await;
    let mut access_check = tokio::time::interval_at(Instant::now() + check_every, check_every);
    access_check.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let cursor = Cursor {
        state,
        headers,
        principal,
        scope,
        opening: Some(opening_event(&subscription)),
        subscription,
        access_check,
    };
    let stream = futures_util::stream::unfold(cursor, Cursor::next);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEP_ALIVE_INTERVAL)))
}

/// Flux unique de l'interface : tâches et sujets à relire, selon les droits de l'appelant.
pub async fn all(
    State(s): State<AppState>,
    headers: HeaderMap,
    AnyAuth(principal): AnyAuth,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    open(s, headers, principal, Scope::All, ACCESS_CHECK_INTERVAL).await
}

/// Flux des tâches seules, pour les clients existants.
pub async fn jobs(
    State(s): State<AppState>,
    headers: HeaderMap,
    ViewJobsAuth(principal): ViewJobsAuth,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    open(s, headers, principal, Scope::Jobs, ACCESS_CHECK_INTERVAL).await
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
