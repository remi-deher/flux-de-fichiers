//! Conversation de l'assistant en tâche de fond : lancement, suivi SSE des évènements et étapes affichées.

use crate::api::{self, ApiError};
use flux_shared::ai::{AiChatRequest, AiRunEvent, AiRunStarted, AiToolCallTrace};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{EventSource, MessageEvent};

const RUNS_PATH: &str = "/api/v1/ai/runs";

fn events_path(run_id: &str) -> String {
    format!("{RUNS_PATH}/{run_id}/events")
}

fn run_path(run_id: &str) -> String {
    format!("{RUNS_PATH}/{run_id}")
}

/// Étape d'outil affichée pendant la réponse : en cours tant que `trace` est vide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveStep {
    pub tool_name: String,
    pub trace: Option<AiToolCallTrace>,
}

/// Met à jour la liste des étapes avec un évènement du flux.
pub fn apply_event(steps: &mut Vec<LiveStep>, event: &AiRunEvent) {
    match event {
        AiRunEvent::ToolStarted { tool_name } => steps.push(LiveStep {
            tool_name: tool_name.clone(),
            trace: None,
        }),
        AiRunEvent::ToolFinished { trace } => {
            let open = steps
                .iter_mut()
                .find(|step| step.trace.is_none() && step.tool_name == trace.tool_name);
            match open {
                Some(step) => step.trace = Some(trace.clone()),
                None => steps.push(LiveStep {
                    tool_name: trace.tool_name.clone(),
                    trace: Some(trace.clone()),
                }),
            }
        }
        AiRunEvent::Done { .. } | AiRunEvent::Failed { .. } => {}
    }
}

/// Lance la conversation ; renvoie l'identifiant du run à suivre.
pub async fn start(request: &AiChatRequest) -> Result<String, ApiError> {
    api::post::<AiChatRequest, AiRunStarted>(RUNS_PATH, Some(request))
        .await
        .map(|started| started.run_id)
}

/// Interrompt un run en cours ; l'échec éventuel n'a pas de conséquence pour l'utilisateur.
pub async fn cancel(run_id: &str) {
    let _ = api::delete(&run_path(run_id)).await;
}

/// Suit les évènements d'un run jusqu'à son évènement final, puis ferme la connexion.
///
/// `on_lost` est appelé si le flux est définitivement perdu (run expiré, session terminée).
pub fn follow(run_id: &str, on_event: impl Fn(AiRunEvent) + 'static, on_lost: impl Fn() + 'static) {
    let Ok(source) = EventSource::new(&events_path(run_id)) else {
        on_lost();
        return;
    };
    let on_message = {
        let source = source.clone();
        Closure::<dyn FnMut(MessageEvent)>::new(move |message: MessageEvent| {
            let Some(event) = message
                .data()
                .as_string()
                .and_then(|data| serde_json::from_str::<AiRunEvent>(&data).ok())
            else {
                return;
            };
            if event.is_final() {
                source.close();
            }
            on_event(event);
        })
    };
    source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    let on_error = {
        let source = source.clone();
        Closure::<dyn FnMut()>::new(move || {
            // Une coupure passagère se reconnecte seule (`Last-Event-ID`) ; seul l'abandon est signalé.
            if source.ready_state() == EventSource::CLOSED {
                on_lost();
            }
        })
    };
    source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    // Les rappels vivent autant que la connexion, fermée à l'évènement final.
    on_message.forget();
    on_error.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(name: &str, outcome: &str) -> AiToolCallTrace {
        AiToolCallTrace {
            tool_name: name.into(),
            read_only: true,
            outcome: outcome.into(),
            duration_ms: 12,
        }
    }

    #[test]
    fn a_finished_tool_closes_the_matching_open_step() {
        let mut steps = Vec::new();
        for name in ["list_workers", "list_workers", "get_job_stats"] {
            apply_event(
                &mut steps,
                &AiRunEvent::ToolStarted {
                    tool_name: name.into(),
                },
            );
        }
        apply_event(
            &mut steps,
            &AiRunEvent::ToolFinished {
                trace: trace("list_workers", "ok"),
            },
        );
        assert_eq!(steps.len(), 3);
        assert!(steps[0].trace.is_some());
        assert!(steps[1].trace.is_none());
        assert!(steps[2].trace.is_none());
    }

    #[test]
    fn a_finish_without_start_still_appears_and_final_events_change_nothing() {
        let mut steps = Vec::new();
        apply_event(
            &mut steps,
            &AiRunEvent::ToolFinished {
                trace: trace("audit", "error"),
            },
        );
        apply_event(
            &mut steps,
            &AiRunEvent::Failed {
                message: "x".into(),
            },
        );
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].trace.as_ref().map(|t| t.outcome.as_str()),
            Some("error")
        );
    }

    #[test]
    fn paths_target_the_run() {
        assert_eq!(events_path("r1"), "/api/v1/ai/runs/r1/events");
        assert_eq!(run_path("r1"), "/api/v1/ai/runs/r1");
    }
}
