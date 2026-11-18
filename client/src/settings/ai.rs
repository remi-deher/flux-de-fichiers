//! Réglages de l'assistant IA intégré (Ollama, OpenAI/compatible, Anthropic Claude).

use super::ai_model::AiModelField;
use super::ai_oauth::AiOauthPanel;
use super::ai_status::AiStatusPanel;
use crate::api;
use crate::ui::{SectionHeader, Tag, TagTone, Toasts};
use flux_shared::ai::{
    AiModelOption, AiProvider, AiSettingsView, AiTestRequest, AiTestResponse,
    DEFAULT_MAX_TOOL_ROUNDS, MAX_TOOL_ROUNDS, MIN_TOOL_ROUNDS, UpdateAiSettingsRequest,
};
use leptos::ev;
use leptos::prelude::*;

const AI_SETTINGS_PATH: &str = "/api/v1/ai/settings";
const AI_TEST_PATH: &str = "/api/v1/ai/test";

fn parse_provider(raw: &str) -> AiProvider {
    AiProvider::parse(raw).unwrap_or(AiProvider::Ollama)
}

#[component]
pub fn AiSettingsSection() -> impl IntoView {
    let reload = RwSignal::new(0u64);
    let resource = LocalResource::new(move || {
        let _ = reload.get();
        api::get::<AiSettingsView>(AI_SETTINGS_PATH)
    });

    view! {
        <section class="settings-section section-panel">
            <SectionHeader
                title="Assistant IA intégré"
                subtitle="Configurez un modèle local (Ollama) ou une API externe (OpenAI, Anthropic) ou votre abonnement ChatGPT par connexion OAuth ; clés et jetons sont chiffrés au repos et jamais exposés au navigateur."
            />
            {move || match resource.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(err)) => view! { <p class="form-error">{err.message()}</p> }.into_any(),
                Some(Ok(initial)) => view! {
                    <AiSettingsForm
                        initial=initial
                        on_saved=Callback::new(move |()| reload.update(|n| *n = n.wrapping_add(1)))
                    />
                }.into_any(),
            }}
        </section>
        <AiStatusPanel />
    }
}

#[component]
fn AiSettingsForm(initial: AiSettingsView, on_saved: Callback<()>) -> impl IntoView {
    let enabled = RwSignal::new(initial.enabled);
    let provider = RwSignal::new(initial.provider);
    let base_url = RwSignal::new(initial.base_url.clone());
    let model = RwSignal::new(initial.model.clone());
    let api_key = RwSignal::new(String::new());
    let clear_api_key = RwSignal::new(false);
    let allow_write_tools = RwSignal::new(initial.allow_write_tools);
    let max_tool_rounds = RwSignal::new(initial.max_tool_rounds.to_string());
    let feedback = RwSignal::new(None::<String>);
    let test_result = RwSignal::new(None::<Result<AiTestResponse, String>>);
    // Modèles proposés par le fournisseur pour la connexion en cours de saisie.
    let catalog = RwSignal::new(Vec::<AiModelOption>::new());
    // Raison pour laquelle la liste est vide (affichée sous le champ du modèle).
    let catalog_issue = RwSignal::new(None::<String>);
    // Le diagnostic n'est montré que pour un test demandé ; le chargement de la liste est silencieux.
    let announce_test = RwSignal::new(false);
    let api_key_set = initial.api_key_set;
    let oauth_account = RwSignal::new(initial.oauth_account.clone());
    let uses_oauth = move || provider.get().uses_oauth();

    let save = Action::new_local(move |req: &UpdateAiSettingsRequest| {
        let req = req.clone();
        async move { api::put::<UpdateAiSettingsRequest, AiSettingsView>(AI_SETTINGS_PATH, &req).await }
    });

    Effect::new(move |_| {
        if let Some(result) = save.value().get() {
            match result {
                Ok(_) => {
                    feedback.set(None);
                    api_key.set(String::new());
                    clear_api_key.set(false);
                    Toasts::notify_success("Réglages de l'assistant IA enregistrés.");
                    on_saved.run(());
                }
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
        }
    });

    let test = Action::new_local(move |req: &AiTestRequest| {
        let req = req.clone();
        async move { api::post::<AiTestRequest, AiTestResponse>(AI_TEST_PATH, Some(&req)).await }
    });

    Effect::new(move |_| {
        if let Some(result) = test.value().get() {
            let announce = announce_test.get_untracked();
            match result {
                Ok(res) => {
                    catalog_issue.set((!res.reachable).then(|| res.message.clone()));
                    catalog.set(if res.reachable {
                        res.available_models.clone()
                    } else {
                        Vec::new()
                    });
                    if announce {
                        test_result.set(Some(Ok(res)));
                    }
                }
                Err(err) => {
                    catalog_issue.set(Some(err.message().to_owned()));
                    catalog.set(Vec::new());
                    if announce {
                        test_result.set(Some(Err(err.message())));
                    }
                }
            }
        }
    });

    let build_api_key_payload = move || {
        if clear_api_key.get_untracked() {
            Some(String::new())
        } else {
            let entered = api_key.get_untracked();
            let trimmed = entered.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
    };

    let test_request = move || AiTestRequest {
        provider: provider.get_untracked(),
        base_url: base_url.get_untracked().trim().to_owned(),
        model: model.get_untracked().trim().to_owned(),
        api_key: build_api_key_payload(),
    };

    // Inutile d'interroger un fournisseur dont on n'a pas encore les identifiants.
    let can_list_models = move || {
        let current = provider.get_untracked();
        if current.uses_oauth() {
            oauth_account.get_untracked().is_some()
        } else if current.requires_api_key() {
            api_key_set || !api_key.get_untracked().trim().is_empty()
        } else {
            true
        }
    };
    let load_models = move || {
        if can_list_models() && !test.pending().get_untracked() {
            announce_test.set(false);
            test.dispatch(test_request());
        }
    };
    // Au chargement, au changement de fournisseur et à la connexion d'un compte.
    Effect::new(move |_| {
        provider.track();
        oauth_account.track();
        load_models();
    });

    let on_provider_change = move |ev| {
        let next = parse_provider(&event_target_value(&ev));
        let prev = provider.get_untracked();
        provider.set(next);
        let cur_url = base_url.get_untracked();
        if cur_url.trim().is_empty() || cur_url.trim() == prev.default_base_url() {
            base_url.set(next.default_base_url().to_owned());
        }
        let cur_model = model.get_untracked();
        if cur_model.trim().is_empty() || cur_model.trim() == prev.default_model() {
            model.set(next.default_model().to_owned());
        }
        test_result.set(None);
        catalog.set(Vec::new());
    };

    let submit = move |ev: ev::SubmitEvent| {
        ev.prevent_default();
        let rounds: u8 = max_tool_rounds
            .get_untracked()
            .trim()
            .parse()
            .ok()
            .filter(|val| (MIN_TOOL_ROUNDS..=MAX_TOOL_ROUNDS).contains(val))
            .unwrap_or(DEFAULT_MAX_TOOL_ROUNDS);
        let req = UpdateAiSettingsRequest {
            enabled: enabled.get_untracked(),
            provider: provider.get_untracked(),
            base_url: base_url.get_untracked().trim().to_owned(),
            model: model.get_untracked().trim().to_owned(),
            api_key: build_api_key_payload(),
            allow_write_tools: allow_write_tools.get_untracked(),
            max_tool_rounds: rounds,
        };
        if req.base_url.is_empty() || (req.model.is_empty() && !req.provider.has_automatic_model())
        {
            Toasts::set_error(
                feedback,
                "L'URL de l'API et le nom du modèle sont obligatoires.",
            );
            return;
        }
        save.dispatch(req);
    };

    view! {
        <form class="settings-form" on:submit=submit>
            <fieldset class="settings-fieldset">
                <legend class="settings-fieldset__legend">"Fournisseur et modèle"</legend>
                <label class="flow-settings__toggle">
                    <input
                        type="checkbox"
                        prop:checked=move || enabled.get()
                        on:change=move |ev| enabled.set(event_target_checked(&ev))
                    />
                    <span>"Activer l'assistant IA dans la barre supérieure et les tiroirs"</span>
                </label>

                <div class="settings-form-grid">
                    <div class="form-group">
                        <label for="ai-provider" class="form-label">"Fournisseur"</label>
                        <select
                            id="ai-provider"
                            class="form-input"
                            prop:value=move || provider.get().as_str()
                            on:change=on_provider_change
                        >
                            {AiProvider::ALL.into_iter().map(|item| view! {
                                <option value=item.as_str()>{item.label()}</option>
                            }).collect_view()}
                        </select>
                    </div>

                    <div class="form-group">
                        <label for="ai-base-url" class="form-label">"URL de base"</label>
                        <input
                            id="ai-base-url"
                            class="form-input text-mono"
                            placeholder=move || provider.get().default_base_url()
                            prop:value=move || base_url.get()
                            on:input=move |ev| base_url.set(event_target_value(&ev))
                        />
                    </div>

                    <AiModelField
                        model=model
                        catalog=catalog
                        issue=catalog_issue
                        provider=provider
                        loading=Signal::derive(move || test.pending().get())
                    />

                    <Show when=move || !uses_oauth()>
                    <div class="form-group">
                        <label for="ai-api-key" class="form-label">
                            <span>"Clé d'API "</span>
                            {api_key_set.then(|| view! {
                                <Tag label="Enregistrée" tone=TagTone::Success />
                            })}
                        </label>
                        <input
                            id="ai-api-key"
                            class="form-input"
                            type="password"
                            autocomplete="new-password"
                            placeholder=move || if api_key_set {
                                "•••• (conservée si vide)"
                            } else if provider.get().requires_api_key() {
                                "sk-…"
                            } else {
                                "Optionnelle pour Ollama local"
                            }
                            disabled=move || clear_api_key.get()
                            prop:value=move || api_key.get()
                            on:input=move |ev| api_key.set(event_target_value(&ev))
                            on:change=move |_| load_models()
                        />
                    </div>
                    </Show>
                </div>

                <Show when=uses_oauth>
                    <AiOauthPanel account=oauth_account />
                </Show>

                <Show when=move || api_key_set && !uses_oauth()>
                    <label class="flow-settings__toggle">
                        <input
                            type="checkbox"
                            prop:checked=move || clear_api_key.get()
                            on:change=move |ev| clear_api_key.set(event_target_checked(&ev))
                        />
                        <span>"Effacer la clé d'API enregistrée"</span>
                    </label>
                </Show>
            </fieldset>

            <fieldset class="settings-fieldset">
                <legend class="settings-fieldset__legend">"Sécurité et autonomie des outils MCP"</legend>
                <label class="flow-settings__toggle">
                    <input
                        type="checkbox"
                        prop:checked=move || allow_write_tools.get()
                        on:change=move |ev| allow_write_tools.set(event_target_checked(&ev))
                    />
                    <span>"Autoriser les outils de création et modification, chaque écriture demandant votre confirmation dans le tiroir (sinon lecture, diagnostic et simulation uniquement)"</span>
                </label>
                <div class="form-group">
                    <label for="ai-max-rounds" class="form-label">"Tours d'outils maximum par question (1–10)"</label>
                    <input
                        id="ai-max-rounds"
                        class="form-input text-mono"
                        type="number"
                        min=MIN_TOOL_ROUNDS
                        max=MAX_TOOL_ROUNDS
                        prop:value=move || max_tool_rounds.get()
                        on:input=move |ev| max_tool_rounds.set(event_target_value(&ev))
                    />
                </div>
            </fieldset>

            {move || test_result.get().map(|res| match res {
                Ok(info) => {
                    let tone = if info.reachable { TagTone::Success } else { TagTone::Danger };
                    let badge = if info.reachable { "Connecté" } else { "Échec" };
                    view! {
                        <div class="diagnostic-box" role="status">
                            <div>
                                <div class="diagnostic-box__title">
                                    <Tag label=badge tone=tone />
                                    <span>{format!(" · {} ms", info.latency_ms)}</span>
                                </div>
                                <div class="diagnostic-box__message">{info.message}</div>
                            </div>
                        </div>
                    }.into_any()
                }
                Err(msg) => view! { <p class="form-error" role="alert">{msg}</p> }.into_any(),
            })}

            {move || feedback.get().map(|err| view! {
                <p class="form-error" role="status">{err}</p>
            })}

            <div class="cluster-end">
                <button
                    type="button"
                    class="btn btn-secondary"
                    disabled=move || test.pending().get()
                    on:click=move |_| {
                        announce_test.set(true);
                        test.dispatch(test_request());
                    }
                >
                    {move || if test.pending().get() { "Test en cours…" } else { "Tester & actualiser les modèles" }}
                </button>
                <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>
                    {move || if save.pending().get() { "Enregistrement…" } else { "Enregistrer" }}
                </button>
            </div>
        </form>
    }
}
