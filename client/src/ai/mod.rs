//! Assistant IA intégré au poste de pilotage (tiroir global, suggestions contextuelles et historique).

mod history;
mod run;

use crate::api;
use crate::route::{Route, use_router};
use crate::ui::{Drawer, Icon, IconName, Tag, TagTone};
use forgearr_shared::Permission;
use forgearr_shared::ai::{
    AiChatMessage, AiChatRequest, AiChatRole, AiClientStatus, AiContext, AiContextKind,
    AiContextObject, AiRunEvent, AiToolCallTrace,
};
use forgearr_shared::session::Me;
use leptos::ev;
use leptos::prelude::*;

const AI_STATUS_PATH: &str = "/api/v1/ai/status";
const HEADER_ICON_SIZE: u32 = 14;

/// État global du tiroir assistant IA, partagé via le contexte Leptos sur le central.
#[derive(Clone, Copy)]
pub struct AiAssistantState {
    pub open: RwSignal<bool>,
    pending: RwSignal<Option<(String, Option<String>)>>,
}

impl AiAssistantState {
    pub fn install() -> Self {
        let state = Self {
            open: RwSignal::new(false),
            pending: RwSignal::new(None),
        };
        provide_context(state);
        state
    }

    pub fn show(self) {
        self.open.set(true);
    }

    pub fn toggle(self) {
        self.open.update(|value| *value = !*value);
    }

    /// Ouvre le tiroir et envoie immédiatement une question ciblée avec son contexte.
    pub fn ask(self, prompt: impl Into<String>, context_hint: Option<String>) {
        self.pending.set(Some((prompt.into(), context_hint)));
        self.open.set(true);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimelineItem {
    role: AiChatRole,
    content: String,
    tool_calls: Vec<AiToolCallTrace>,
}

/// Blocs typés extraits d'une réponse texte pour l'affichage sûr sans HTML brut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageBlock {
    Paragraph(String),
    BulletList(Vec<String>),
    CodeBlock(String),
}

pub fn parse_message_blocks(raw: &str) -> Vec<MessageBlock> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut bullets = Vec::new();
    let mut code_lines = Vec::new();
    let mut in_code = false;

    let flush_paragraph = |buf: &mut Vec<String>, out: &mut Vec<MessageBlock>| {
        if !buf.is_empty() {
            out.push(MessageBlock::Paragraph(buf.join("\n")));
            buf.clear();
        }
    };
    let flush_bullets = |buf: &mut Vec<String>, out: &mut Vec<MessageBlock>| {
        if !buf.is_empty() {
            out.push(MessageBlock::BulletList(std::mem::take(buf)));
        }
    };

    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().starts_with("```") {
            if in_code {
                blocks.push(MessageBlock::CodeBlock(code_lines.join("\n")));
                code_lines.clear();
                in_code = false;
            } else {
                flush_paragraph(&mut paragraph, &mut blocks);
                flush_bullets(&mut bullets, &mut blocks);
                in_code = true;
            }
            continue;
        }
        if in_code {
            code_lines.push(trimmed.to_owned());
            continue;
        }
        if trimmed.trim().is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            flush_bullets(&mut bullets, &mut blocks);
            continue;
        }
        let bullet_text = trimmed
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| trimmed.trim_start().strip_prefix("* "));
        if let Some(item) = bullet_text {
            flush_paragraph(&mut paragraph, &mut blocks);
            bullets.push(item.to_owned());
        } else {
            flush_bullets(&mut bullets, &mut blocks);
            paragraph.push(trimmed.to_owned());
        }
    }
    if in_code && !code_lines.is_empty() {
        blocks.push(MessageBlock::CodeBlock(code_lines.join("\n")));
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    flush_bullets(&mut bullets, &mut blocks);
    blocks
}

/// Raccourcis de questions proposés selon l'écran actif.
pub fn starter_prompts(route: &Route) -> Vec<(&'static str, String)> {
    match route {
        Route::Jobs => vec![
            (
                "Analyser les échecs récents",
                "Liste les tâches en échec récentes et diagnostique les causes probables.".to_owned(),
            ),
            (
                "Statistiques de la file",
                "Donne un résumé des statistiques des tâches en attente, en cours et terminées.".to_owned(),
            ),
        ],
        Route::FlowEditor(id) => vec![
            (
                "Vérifier et simuler ce flux",
                format!("Inspecte, valide et simule le flux `{id}` pour vérifier qu'il est sain."),
            ),
            (
                "Comparer les dernières versions",
                format!("Compare les deux dernières versions du flux `{id}` avec diff_flow_versions."),
            ),
        ],
        Route::Flows | Route::NewFlow => vec![
            (
                "Auditer les flux",
                "Liste tous les flux configurés, vérifie leur validité et signale les anomalies éventuelles.".to_owned(),
            ),
            (
                "Blocs disponibles",
                "Quels blocs de traitement et d'extension sont disponibles pour bâtir un flux ?".to_owned(),
            ),
        ],
        Route::Libraries | Route::NewScript | Route::ScriptEditor(_) => vec![
            (
                "Auditer les bibliothèques",
                "Analyse la configuration des bibliothèques (filtres, flux associés, règles de sortie).".to_owned(),
            ),
            (
                "Fichiers en attente",
                "Y a-t-il des fichiers source détectés qui n'ont pas encore de tâche associée ?".to_owned(),
            ),
        ],
        Route::Workers => vec![
            (
                "Santé des workers",
                "Vérifie l'état des workers, leur charge CPU/RAM/GPU et les éventuelles demandes d'enrôlement.".to_owned(),
            ),
        ],
        Route::Plugins => vec![
            (
                "État des extensions",
                "Liste les extensions et outils installés ou manquants dans le catalogue.".to_owned(),
            ),
        ],
        Route::Overview | Route::Settings => vec![
            (
                "Bilan du central",
                "Fais un point rapide sur les statistiques de tâches, les workers en ligne et les anomalies éventuelles.".to_owned(),
            ),
            (
                "Diagnostiquer les échecs",
                "Y a-t-il des tâches en échec récemment ? Si oui, analyse la plus récente.".to_owned(),
            ),
        ],
    }
}

/// Écran et objet ouverts, joints à chaque question pour que l'assistant sache de quoi on parle.
fn route_context(route: &Route) -> AiContext {
    let object = match route {
        Route::FlowEditor(id) => Some(AiContextObject {
            kind: AiContextKind::Flow,
            id: id.clone(),
        }),
        _ => None,
    };
    AiContext {
        screen: route.path(),
        object,
    }
}

pub(crate) fn render_blocks(text: &str) -> AnyView {
    let blocks = parse_message_blocks(text);
    if blocks.is_empty() {
        return view! { <p class="text-dim">"(Réponse vide)"</p> }.into_any();
    }
    view! {
        <>
            {blocks.into_iter().map(|block| match block {
                MessageBlock::Paragraph(p) => view! { <p>{p}</p> }.into_any(),
                MessageBlock::BulletList(items) => view! {
                    <ul>
                        {items.into_iter().map(|item| view! { <li>{item}</li> }).collect_view()}
                    </ul>
                }.into_any(),
                MessageBlock::CodeBlock(code) => view! {
                    <pre class="ai-chat__code"><code>{code}</code></pre>
                }.into_any(),
            }).collect_view()}
        </>
    }
    .into_any()
}

#[component]
pub fn AiAssistantDrawer(me: Me) -> impl IntoView {
    let state = use_context::<AiAssistantState>().unwrap_or_else(AiAssistantState::install);
    let router = use_router();
    let messages = RwSignal::new(Vec::<TimelineItem>::new());
    let input = RwSignal::new(String::new());
    let error_msg = RwSignal::new(None::<String>);
    let reload_status = RwSignal::new(0u64);
    let can_configure = me.allows(Permission::ManageSettings);

    Effect::new(move |_| {
        if state.open.get() {
            reload_status.update(|n| *n = n.wrapping_add(1));
        }
    });

    let status_resource = LocalResource::new(move || {
        let _ = reload_status.get();
        async move { api::get::<AiClientStatus>(AI_STATUS_PATH).await }
    });

    let pending = RwSignal::new(false);
    let steps = RwSignal::new(Vec::<run::LiveStep>::new());
    let run_id = RwSignal::new(None::<String>);
    let confirmation = RwSignal::new(None::<run::PendingAction>);
    // Réponse en cours d'écriture, affichée au fil de l'eau.
    let streamed = RwSignal::new(String::new());
    // Conversation de l'historique en cours (créée par le central au premier échange).
    let conversation_id = RwSignal::new(None::<String>);
    let history_open = RwSignal::new(false);
    let history_reload = RwSignal::new(0u64);

    // Termine la conversation en cours (réponse, échec ou perte du flux).
    let finish = move |outcome: Result<forgearr_shared::ai::AiChatResponse, String>| {
        steps.set(Vec::new());
        streamed.set(String::new());
        confirmation.set(None);
        run_id.set(None);
        pending.set(false);
        match outcome {
            Ok(reply) => {
                if reply.conversation_id.is_some() {
                    conversation_id.set(reply.conversation_id);
                    history_reload.update(|n| *n = n.wrapping_add(1));
                }
                messages.update(|list| {
                    list.push(TimelineItem {
                        role: AiChatRole::Assistant,
                        content: reply.reply,
                        tool_calls: reply.tool_calls,
                    });
                });
            }
            Err(message) => error_msg.set(Some(message)),
        }
    };

    let send = move |text: String, hint: Option<String>| {
        let user_text = text.trim().to_owned();
        if pending.get_untracked() || user_text.is_empty() {
            return;
        }
        let mut wire_messages: Vec<AiChatMessage> = messages.with_untracked(|items| {
            items
                .iter()
                .map(|item| AiChatMessage {
                    role: item.role,
                    content: item.content.clone(),
                })
                .collect()
        });
        wire_messages.push(AiChatMessage {
            role: AiChatRole::User,
            content: user_text.clone(),
        });
        messages.update(|list| {
            list.push(TimelineItem {
                role: AiChatRole::User,
                content: user_text,
                tool_calls: Vec::new(),
            });
        });
        error_msg.set(None);
        pending.set(true);
        let request = AiChatRequest {
            messages: history::wire_window(wire_messages),
            conversation_id: conversation_id.get_untracked(),
            context_hint: hint,
            context: Some(route_context(&router.current())),
        };
        leptos::task::spawn_local(async move {
            let id = match run::start(&request).await {
                Ok(id) => id,
                Err(err) => {
                    let message = if err.code() == Some("conflict") {
                        "L'assistant IA n'est pas encore activé ou configuré.".to_owned()
                    } else {
                        err.message()
                    };
                    finish(Err(message));
                    return;
                }
            };
            run_id.set(Some(id.clone()));
            let current = id.clone();
            let is_current =
                move || run_id.try_get_untracked().flatten().as_deref() == Some(&current);
            let lost_is_current = is_current.clone();
            run::follow(
                &id,
                move |event| {
                    if !is_current() {
                        return;
                    }
                    match event {
                        AiRunEvent::Done { response } => finish(Ok(response)),
                        AiRunEvent::Failed { message } => finish(Err(message)),
                        progress => {
                            streamed.update(|text| {
                                *text = run::streamed_after(std::mem::take(text), &progress);
                            });
                            confirmation.update(|current| {
                                *current = run::pending_after(current.take(), &progress);
                            });
                            steps.update(|list| run::apply_event(list, &progress));
                        }
                    }
                },
                move || {
                    if lost_is_current() {
                        finish(Err(
                            "Connexion à la conversation perdue : réessayez.".to_owned()
                        ));
                    }
                },
            );
        });
    };

    let stop = move |_| {
        if let Some(id) = run_id.get_untracked() {
            leptos::task::spawn_local(async move { run::cancel(&id).await });
        }
    };

    // Autorise ou refuse l'action en attente ; le run reprend dès que le central a la réponse.
    let answer = move |approve: bool| {
        let (Some(id), Some(action)) = (run_id.get_untracked(), confirmation.get_untracked())
        else {
            return;
        };
        confirmation.set(None);
        leptos::task::spawn_local(async move {
            if let Err(err) = run::decide(&id, &action.action_id, approve).await {
                error_msg.set(Some(err.message()));
            }
        });
    };

    Effect::new(move |_| {
        if state.open.get()
            && let Some((prompt, custom_hint)) = state.pending.get()
        {
            state.pending.set(None);
            send(prompt, custom_hint);
        }
    });

    let submit_current = move || {
        let text = input.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        input.set(String::new());
        send(text, None);
    };

    let new_conversation = move || {
        messages.set(Vec::new());
        error_msg.set(None);
        conversation_id.set(None);
    };
    let open_conversation = Callback::new(move |id: String| {
        leptos::task::spawn_local(async move {
            match history::load(&id).await {
                Ok(conversation) => {
                    messages.set(
                        conversation
                            .messages
                            .into_iter()
                            .map(|message| TimelineItem {
                                role: message.role,
                                content: message.content,
                                tool_calls: Vec::new(),
                            })
                            .collect(),
                    );
                    error_msg.set(None);
                    conversation_id.set(Some(conversation.id));
                    history_open.set(false);
                }
                Err(err) => error_msg.set(Some(err.message())),
            }
        });
    });

    let header_actions = view! {
        <div class="cluster">
            <button
                type="button"
                class="btn btn-secondary btn-sm"
                aria-pressed=move || history_open.get().to_string()
                on:click=move |_| history_open.update(|open| *open = !*open)
            >
                "Historique"
            </button>
            <button
                type="button"
                class="btn btn-secondary btn-sm"
                disabled=move || pending.get()
                    || (messages.with(Vec::is_empty) && error_msg.get().is_none())
                on:click=move |_| new_conversation()
            >
                "Nouvelle"
            </button>
        </div>
    }
    .into_any();

    view! {
        <Drawer
            open=state.open
            eyebrow="Copilote Forgearr"
            title="Assistant IA"
            actions=header_actions
        >
            <div class="ai-chat">
                {move || match status_resource.get() {
                    None => view! {
                        <div class="ai-chat__status">
                            <span class="ai-chat__status-info">"Vérification de l'assistant…"</span>
                        </div>
                    }.into_any(),
                    Some(Err(err)) => view! {
                        <div class="ai-chat__status">
                            <span class="ai-chat__status-info">{err.message()}</span>
                        </div>
                    }.into_any(),
                    Some(Ok(st)) if !st.available => view! {
                        <div class="ai-chat__status">
                            <div class="ai-chat__status-info">
                                <Tag label="Désactivé" tone=TagTone::Neutral />
                                <span>"Configurez Ollama, OpenAI ou Anthropic pour activer l'assistant."</span>
                            </div>
                            {can_configure.then(|| view! {
                                <button
                                    type="button"
                                    class="btn btn-primary btn-sm"
                                    on:click=move |_| {
                                        state.open.set(false);
                                        router.go(Route::Settings);
                                    }
                                >
                                    "Paramètres"
                                </button>
                            })}
                        </div>
                    }.into_any(),
                    Some(Ok(st)) => {
                        let mode_label = if st.allow_write_tools {
                            "Lecture + actions"
                        } else {
                            "Lecture seule"
                        };
                        let mode_tone = if st.allow_write_tools {
                            TagTone::Warning
                        } else {
                            TagTone::Success
                        };
                        view! {
                            <div class="ai-chat__status">
                                <div class="ai-chat__status-info">
                                    <Icon name=IconName::Sparkles size=HEADER_ICON_SIZE />
                                    <strong>{st.provider.label()}</strong>
                                    <span class="text-mono">{st.model}</span>
                                </div>
                                <Tag label=mode_label tone=mode_tone />
                            </div>
                        }.into_any()
                    }
                }}

                <Show when=move || history_open.get()>
                    <history::AiHistoryPanel
                        reload=history_reload
                        current=conversation_id
                        on_open=open_conversation
                        on_current_deleted=Callback::new(move |()| new_conversation())
                    />
                </Show>

                <div class="ai-chat__prompts" role="group" aria-label="Suggestions contextuelles">
                    {move || {
                        let current = router.current();
                        let prompts = starter_prompts(&current);
                        let is_ready = status_resource
                            .get()
                            .and_then(Result::ok)
                            .is_some_and(|st| st.available);
                        prompts.into_iter().map(move |(label, prompt)| {
                            let prompt_copy = prompt.clone();
                            view! {
                                <button
                                    type="button"
                                    class="ai-chat__chip"
                                    disabled=move || !is_ready || pending.get()
                                    on:click=move |_| send(prompt_copy.clone(), None)
                                >
                                    {label}
                                </button>
                            }
                        }).collect_view()
                    }}
                </div>

                <div class="ai-chat__messages" role="log" aria-live="polite">
                    {move || {
                        let items = messages.get();
                        if items.is_empty() && !pending.get() {
                            return view! {
                                <p class="text-dim text-sm">
                                    "Posez une question sur vos tâches, flux, bibliothèques ou workers."
                                </p>
                            }.into_any();
                        }
                        view! {
                            <>
                                {items.into_iter().map(|item| {
                                    let is_user = item.role == AiChatRole::User;
                                    let role_label = if is_user { "Vous" } else { "Assistant" };
                                    let calls = item.tool_calls;
                                    view! {
                                        <article
                                            class="ai-chat__msg"
                                            class:ai-chat__msg--user=is_user
                                            class:ai-chat__msg--assistant={!is_user}
                                        >
                                            <span class="ai-chat__msg-role">{role_label}</span>
                                            <div class="ai-chat__msg-body">
                                                {render_blocks(&item.content)}
                                            </div>
                                            {(!calls.is_empty()).then(|| view! {
                                                <div class="ai-chat__tools" aria-label="Outils consultés">
                                                    {calls.into_iter().map(|call| {
                                                        let is_err = call.outcome != "ok";
                                                        view! {
                                                            <span class="ai-chat__tool-pill" class:is-error=is_err>
                                                                <span>{call.tool_name}</span>
                                                                <span>{format!("{} · {} ms", call.outcome, call.duration_ms)}</span>
                                                            </span>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            })}
                                        </article>
                                    }
                                }).collect_view()}
                                {pending.get().then(|| view! {
                                    <article class="ai-chat__msg ai-chat__msg--assistant">
                                        <span class="ai-chat__msg-role">"Assistant"</span>
                                        {move || {
                                            let text = streamed.get();
                                            if text.trim().is_empty() {
                                                view! {
                                                    <p class="text-dim text-sm">
                                                        {if confirmation.get().is_some() {
                                                            "En attente de votre autorisation…"
                                                        } else {
                                                            "Analyse en cours…"
                                                        }}
                                                    </p>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="ai-chat__msg-body">{render_blocks(&text)}</div>
                                                }.into_any()
                                            }
                                        }}
                                        {move || confirmation.get().map(|action| view! {
                                            <div class="diagnostic-box" role="alertdialog" aria-label="Autoriser l'action">
                                                <div>
                                                    <div class="diagnostic-box__title">
                                                        {format!("Autoriser l'action « {} » ?", action.tool_name)}
                                                    </div>
                                                    <pre class="ai-chat__code"><code>{action.summary}</code></pre>
                                                    <div class="cluster">
                                                        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| answer(true)>
                                                            "Autoriser"
                                                        </button>
                                                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| answer(false)>
                                                            "Refuser"
                                                        </button>
                                                    </div>
                                                </div>
                                            </div>
                                        })}
                                        <div class="ai-chat__tools" aria-label="Outils en cours">
                                            {move || steps.get().into_iter().map(|step| {
                                                let (state_label, is_err) = match &step.trace {
                                                    None => ("en cours…".to_owned(), false),
                                                    Some(t) => (format!("{} · {} ms", t.outcome, t.duration_ms), t.outcome != "ok"),
                                                };
                                                view! {
                                                    <span class="ai-chat__tool-pill" class:is-error=is_err>
                                                        <span>{step.tool_name}</span>
                                                        <span>{state_label}</span>
                                                    </span>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </article>
                                })}
                            </>
                        }.into_any()
                    }}
                </div>

                {move || error_msg.get().map(|err| view! {
                    <p class="form-error" role="alert">{err}</p>
                })}

                <form
                    class="ai-chat__composer"
                    on:submit=move |ev: ev::SubmitEvent| {
                        ev.prevent_default();
                        submit_current();
                    }
                >
                    <textarea
                        class="form-input ai-chat__textarea"
                        placeholder="Ex. : Pourquoi la dernière tâche a échoué ?"
                        disabled=move || {
                            pending.get()
                                || !status_resource
                                    .get()
                                    .and_then(Result::ok)
                                    .is_some_and(|st| st.available)
                        }
                        prop:value=move || input.get()
                        on:input=move |ev| input.set(event_target_value(&ev))
                        on:keydown=move |ev: ev::KeyboardEvent| {
                            if ev.key() == "Enter" && !ev.shift_key() {
                                ev.prevent_default();
                                submit_current();
                            }
                        }
                    ></textarea>
                    <div class="cluster-end">
                        <Show when=move || pending.get()>
                            <button type="button" class="btn btn-secondary btn-sm" on:click=stop>
                                "Arrêter"
                            </button>
                        </Show>
                        <button
                            type="submit"
                            class="btn btn-primary btn-sm"
                            disabled=move || {
                                pending.get()
                                    || input.get().trim().is_empty()
                                    || !status_resource
                                        .get()
                                        .and_then(Result::ok)
                                        .is_some_and(|st| st.available)
                            }
                        >
                            {move || if pending.get() { "Réflexion…" } else { "Envoyer" }}
                        </button>
                    </div>
                </form>
            </div>
        </Drawer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message_blocks_handles_paragraphs_lists_and_code() {
        let sample = "Diagnostic initial :\n\n- Erreur mkvpropedit\n* Flux v3\n\n```json\n{\"ok\":true}\n```\nFin.";
        assert_eq!(
            parse_message_blocks(sample),
            vec![
                MessageBlock::Paragraph("Diagnostic initial :".into()),
                MessageBlock::BulletList(vec!["Erreur mkvpropedit".into(), "Flux v3".into()]),
                MessageBlock::CodeBlock("{\"ok\":true}".into()),
                MessageBlock::Paragraph("Fin.".into()),
            ]
        );
    }

    #[test]
    fn starter_prompts_adapt_to_current_route() {
        let editor_prompts = starter_prompts(&Route::FlowEditor("flow.films".into()));
        assert!(editor_prompts[0].1.contains("flow.films"));
        assert!(!starter_prompts(&Route::Jobs).is_empty());
        assert!(!starter_prompts(&Route::Workers).is_empty());
    }
}
