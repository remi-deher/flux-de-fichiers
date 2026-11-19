//! Conversation de l'assistant en tâche de fond : lancement, suivi SSE des évènements et étapes affichées.

use crate::api::{self, ApiError};
use forgearr_shared::ai::{
    AiActionDecision, AiChatRequest, AiRunEvent, AiRunStarted, AiToolCallTrace,
};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{EventSource, MessageEvent};

const RUNS_PATH: &str = "/api/v1/ai/runs";

fn events_path(run_id: &str) -> String {
    format!("{RUNS_PATH}/{run_id}/events")
}

fn run_path(run_id: &str) -> String {
    format!("{RUNS_PATH}/{run_id}")
}

/// Action d'écriture en attente de la réponse de l'utilisateur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingAction {
    pub action_id: String,
    pub tool_name: String,
    pub summary: String,
}

/// Action en attente après `event` : posée par une demande, levée par sa résolution.
pub fn pending_after(current: Option<PendingAction>, event: &AiRunEvent) -> Option<PendingAction> {
    match event {
        AiRunEvent::Confirmation {
            action_id,
            tool_name,
            summary,
        } => Some(PendingAction {
            action_id: action_id.clone(),
            tool_name: tool_name.clone(),
            summary: summary.clone(),
        }),
        AiRunEvent::ActionResolved { action_id, .. } => {
            current.filter(|pending| &pending.action_id != action_id)
        }
        _ => current,
    }
}

/// Texte affiché pendant la réponse après `event` : il s'allonge avec les morceaux reçus et repart
/// de zéro quand un outil démarre (le texte d'un tour qui appelle un outil n'est pas la réponse).
pub fn streamed_after(mut current: String, event: &AiRunEvent) -> String {
    match event {
        AiRunEvent::Delta { text } => current.push_str(text),
        AiRunEvent::ToolStarted { .. } => current.clear(),
        _ => {}
    }
    current
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
        AiRunEvent::Delta { .. }
        | AiRunEvent::Confirmation { .. }
        | AiRunEvent::ActionResolved { .. }
        | AiRunEvent::Done { .. }
        | AiRunEvent::Failed { .. } => {}
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

/// Transmet l'autorisation ou le refus d'une action ; l'erreur éventuelle est renvoyée à l'appelant.
pub async fn decide(run_id: &str, action_id: &str, approve: bool) -> Result<(), ApiError> {
    api::post_empty(
        &format!("{}/actions/{action_id}", run_path(run_id)),
        Some(&AiActionDecision { approve }),
    )
    .await
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
    fn streamed_text_grows_with_deltas_and_restarts_when_a_tool_starts() {
        let delta = |text: &str| AiRunEvent::Delta { text: text.into() };
        let mut text = String::new();
        for event in [delta("Je "), delta("regarde")] {
            text = streamed_after(text, &event);
        }
        assert_eq!(text, "Je regarde");
        text = streamed_after(
            text,
            &AiRunEvent::ToolStarted {
                tool_name: "list_jobs".into(),
            },
        );
        assert!(text.is_empty());
        text = streamed_after(text, &delta("Voici"));
        assert_eq!(text, "Voici");
    }

    #[test]
    fn a_confirmation_stays_pending_until_its_own_resolution() {
        let ask = AiRunEvent::Confirmation {
            action_id: "a1".into(),
            tool_name: "upsert_local_script".into(),
            summary: "{}".into(),
        };
        let pending = pending_after(None, &ask);
        assert_eq!(pending.as_ref().map(|p| p.action_id.as_str()), Some("a1"));
        let other = AiRunEvent::ActionResolved {
            action_id: "zz".into(),
            approved: true,
        };
        assert!(pending_after(pending.clone(), &other).is_some());
        let resolved = AiRunEvent::ActionResolved {
            action_id: "a1".into(),
            approved: false,
        };
        assert_eq!(pending_after(pending, &resolved), None);
    }

    #[test]
    fn paths_target_the_run() {
        assert_eq!(events_path("r1"), "/api/v1/ai/runs/r1/events");
        assert_eq!(run_path("r1"), "/api/v1/ai/runs/r1");
    }
}
