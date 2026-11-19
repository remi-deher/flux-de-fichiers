//! Historique des conversations de l'assistant : liste, reprise et suppression.

use crate::api;
use crate::ui::Toasts;
use forgearr_shared::ai::{
    AiChatMessage, AiChatRole, AiConversation, AiConversationList, MAX_CHAT_MESSAGES,
};
use leptos::prelude::*;

const CONVERSATIONS_PATH: &str = "/api/v1/ai/conversations";

fn conversation_path(id: &str) -> String {
    format!("{CONVERSATIONS_PATH}/{id}")
}

/// Messages envoyés au central : les plus récents seulement, en commençant par une question.
pub fn wire_window(mut messages: Vec<AiChatMessage>) -> Vec<AiChatMessage> {
    if messages.len() > MAX_CHAT_MESSAGES {
        messages.drain(..messages.len() - MAX_CHAT_MESSAGES);
    }
    let first_question = messages
        .iter()
        .position(|message| message.role == AiChatRole::User)
        .unwrap_or(messages.len());
    messages.drain(..first_question);
    messages
}

/// Charge une conversation de l'historique.
pub async fn load(id: &str) -> Result<AiConversation, api::ApiError> {
    api::get::<AiConversation>(&conversation_path(id)).await
}

/// Liste des conversations enregistrées ; ouvrir l'une d'elles appelle `on_open`.
#[component]
pub fn AiHistoryPanel(
    /// Incrémenté pour relire la liste (nouvel échange enregistré).
    reload: RwSignal<u64>,
    /// Conversation actuellement affichée.
    current: RwSignal<Option<String>>,
    on_open: Callback<String>,
    /// Appelé quand la conversation affichée vient d'être supprimée.
    on_current_deleted: Callback<()>,
) -> impl IntoView {
    let list = LocalResource::new(move || {
        reload.track();
        async move { api::get::<AiConversationList>(CONVERSATIONS_PATH).await }
    });
    let feedback = RwSignal::new(None::<String>);

    let remove = move |id: String| {
        leptos::task::spawn_local(async move {
            match api::delete(&conversation_path(&id)).await {
                Ok(()) => {
                    if current.get_untracked().as_deref() == Some(id.as_str()) {
                        on_current_deleted.run(());
                    }
                    reload.update(|n| *n = n.wrapping_add(1));
                }
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
        });
    };
    let remove_all = move |_| {
        leptos::task::spawn_local(async move {
            match api::delete(CONVERSATIONS_PATH).await {
                Ok(()) => {
                    on_current_deleted.run(());
                    reload.update(|n| *n = n.wrapping_add(1));
                }
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
        });
    };

    view! {
        <section class="stack-xs" aria-label="Historique des conversations">
            {move || match list.get() {
                None => view! { <p class="form-help">"Chargement de l'historique…"</p> }.into_any(),
                Some(Err(err)) => view! { <p class="form-error" role="alert">{err.message()}</p> }.into_any(),
                Some(Ok(list)) if list.conversations.is_empty() => view! {
                    <p class="form-help">"Aucune conversation enregistrée."</p>
                }.into_any(),
                Some(Ok(list)) => view! {
                    <div class="stack-xs">
                        {list.conversations.into_iter().map(|item| {
                            let open_id = item.id.clone();
                            let delete_id = item.id.clone();
                            view! {
                                <div class="cluster">
                                    <button
                                        type="button"
                                        class="ai-chat__chip"
                                        on:click=move |_| on_open.run(open_id.clone())
                                    >
                                        {item.title}
                                    </button>
                                    <span class="text-dim text-sm">{item.updated_at}</span>
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm"
                                        aria-label="Supprimer cette conversation"
                                        on:click=move |_| remove(delete_id.clone())
                                    >
                                        "Supprimer"
                                    </button>
                                </div>
                            }
                        }).collect_view()}
                        <div class="cluster-end">
                            <button type="button" class="btn btn-secondary btn-sm" on:click=remove_all>
                                "Tout supprimer"
                            </button>
                        </div>
                    </div>
                }.into_any(),
            }}
            {move || feedback.get().map(|err| view! { <p class="form-error" role="alert">{err}</p> })}
            <p class="form-help">
                "Conservées 30 jours (50 conversations au plus), chiffrées, visibles de vous seul. Les résultats d'outils ne sont pas enregistrés."
            </p>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: AiChatRole, text: &str) -> AiChatMessage {
        AiChatMessage {
            role,
            content: text.into(),
        }
    }

    #[test]
    fn the_window_keeps_recent_messages_and_starts_with_a_question() {
        let mut long = Vec::new();
        for n in 0..(MAX_CHAT_MESSAGES + 3) {
            let role = if n % 2 == 0 {
                AiChatRole::User
            } else {
                AiChatRole::Assistant
            };
            long.push(message(role, &n.to_string()));
        }
        let window = wire_window(long);
        assert!(window.len() <= MAX_CHAT_MESSAGES);
        assert_eq!(window[0].role, AiChatRole::User);
        assert_eq!(
            window.last().map(|m| m.content.as_str()),
            Some((MAX_CHAT_MESSAGES + 2).to_string().as_str())
        );
    }

    #[test]
    fn a_short_conversation_is_untouched_and_paths_target_the_item() {
        let short = vec![
            message(AiChatRole::User, "a"),
            message(AiChatRole::Assistant, "b"),
        ];
        assert_eq!(wire_window(short.clone()), short);
        assert!(wire_window(vec![message(AiChatRole::Assistant, "x")]).is_empty());
        assert_eq!(conversation_path("c1"), "/api/v1/ai/conversations/c1");
    }
}
