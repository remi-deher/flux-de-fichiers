//! Réglages d'un plugin, générés depuis son `settingsSchema` (secrets jamais relus).

use super::metadata::{StatusBanner, StoreTone};
use super::settings_form::{Incoming, SettingsForm};
use crate::api;
use crate::live::{ChangedElsewhere, load, use_live};
use crate::route::encode_segment;
use forgearr_shared::live::ChangeTopic;
use forgearr_shared::plugins::{
    PluginConnectionTestResult, PluginSettingsView, ResolvedSettingsField, SettingsFieldType,
};
use leptos::prelude::*;
use serde_json::{Map, Value};

/// Le schéma des champs vient du catalogue.
const SETTINGS_TOPICS: &[ChangeTopic] = &[ChangeTopic::Settings, ChangeTopic::Catalog];

const SECRET_SET_PLACEHOLDER: &str = "Défini — laisser vide pour conserver";

/// Saisie envoyée, et dernier contenu du central lu à cet instant.
#[derive(Clone)]
struct Submission {
    entries: Vec<(ResolvedSettingsField, String)>,
    seen: Option<PluginSettingsView>,
}

/// Corps envoyé : un secret laissé vide est conservé par le central.
pub fn settings_body(fields: &[(ResolvedSettingsField, String)]) -> Map<String, Value> {
    fields
        .iter()
        .map(|(field, value)| (field.name.clone(), Value::String(value.clone())))
        .collect()
}

fn input_kind(field: &ResolvedSettingsField) -> &'static str {
    match field.field_type {
        SettingsFieldType::Secret => "password",
        SettingsFieldType::Url => "url",
        SettingsFieldType::String => "text",
    }
}

#[component]
pub fn PluginSettings(plugin_id: String) -> impl IntoView {
    let path = format!("/api/v1/plugins/{}/settings", encode_segment(&plugin_id));
    let live = use_live();
    let loaded = load::<PluginSettingsView>(SETTINGS_TOPICS, path.clone());
    let form = RwSignal::new(SettingsForm::default());
    let shape = Memo::new(move |_| form.read().shape());
    let incoming = Memo::new(move |_| match loaded.read().as_ref() {
        Some(Ok(fresh)) => form.read().incoming(fresh),
        _ => Incoming::Keep,
    });
    let fresh = move || loaded.get_untracked().and_then(Result::ok);
    Effect::new(move |_| {
        // Suit aussi `loaded` : deux contenus successifs peuvent être adoptés.
        loaded.track();
        if incoming.get() == Incoming::Adopt
            && let Some(fresh) = fresh()
        {
            form.update(|form| form.adopt(fresh));
        }
    });
    let test_path = format!("/api/v1/plugins/{}/test", encode_segment(&plugin_id));
    let feedback = RwSignal::new(None::<(bool, String)>);
    let test_feedback = RwSignal::new(None::<(bool, String)>);
    let save = Action::new_local(move |submission: &Submission| {
        let (path, submission) = (path.clone(), submission.clone());
        async move {
            api::put::<Map<String, Value>, Value>(&path, &settings_body(&submission.entries))
                .await
                .map(|_| submission)
        }
    });
    let test_conn = Action::new_local(move |body: &Map<String, Value>| {
        let (path, body) = (test_path.clone(), body.clone());
        async move {
            api::post::<Map<String, Value>, PluginConnectionTestResult>(&path, Some(&body)).await
        }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok(submitted)) => {
            feedback.set(Some((true, "Réglages enregistrés.".into())));
            form.update(|form| form.saved(&submitted.entries, submitted.seen));
            live.touch(SETTINGS_TOPICS);
        }
        Some(Err(error)) => feedback.set(Some((false, error.message()))),
        None => {}
    });
    Effect::new(move |_| match test_conn.value().get() {
        Some(Ok(res)) => test_feedback.set(Some((res.ok, res.message))),
        Some(Err(error)) => test_feedback.set(Some((false, error.message()))),
        None => {}
    });
    view! {
        <div class="inspection-panel">
            {move || feedback.get().map(|(ok, msg)| view! {
                <StatusBanner
                    title=if ok { "Enregistrement réussi" } else { "Erreur d'enregistrement" }
                    text=Some(msg)
                    tone=if ok { StoreTone::Success } else { StoreTone::Danger }
                />
            })}
            {move || test_feedback.get().map(|(ok, msg)| view! {
                <StatusBanner
                    title=if ok { "Connexion vérifiée" } else { "Échec de la connexion" }
                    text=Some(msg)
                    tone=if ok { StoreTone::Success } else { StoreTone::Danger }
                />
            })}
            {move || (incoming.get() == Incoming::Warn && !save.pending().get()).then(|| view! {
                <ChangedElsewhere
                    text="Ces réglages ont changé pendant votre saisie. Recharger remplace votre saisie ; enregistrer remplace ces changements."
                    on_reload=Some(Callback::new(move |()| {
                        if let Some(fresh) = fresh() {
                            form.update(|form| form.reset(fresh));
                        }
                    }))
                />
            })}
            {move || match loaded.get() {
                Some(Err(error)) => Some(view! { <p class="text-danger">{error.message()}</p> }),
                _ => None,
            }}
            {move || match shape.get() {
                None if loaded.read().is_some() => ().into_any(),
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(fields) if fields.is_empty() => view! { <p class="text-dim">"Cette extension n'a pas de réglage."</p> }.into_any(),
                Some(fields) => {
                    let has_url_field = fields
                        .iter()
                        .any(|field| field.field_type == SettingsFieldType::Url);
                    view! {
                        <form on:submit=move |event| {
                            event.prevent_default();
                            save.dispatch(Submission {
                                entries: form.read_untracked().entries(),
                                seen: fresh(),
                            });
                        }>
                            {fields.into_iter().enumerate().map(|(index, field)| {
                                let id = format!("plugin-setting-{}", field.name);
                                let placeholder = move || if form.read().is_set(index) { SECRET_SET_PLACEHOLDER } else { "" };
                                let label = if field.required { format!("{} *", field.label) } else { field.label.clone() };
                                view! {
                                    <div class="form-group">
                                        <label for=id.clone() class="form-label">{label}</label>
                                        <input id=id class="form-input" type=input_kind(&field) autocomplete="off" placeholder=placeholder
                                            prop:value=move || form.read().draft(index)
                                            on:input=move |event| form.update(|form| form.type_in(index, event_target_value(&event))) />
                                        {field.description.clone().map(|text| view! { <p class="form-help">{text}</p> })}
                                    </div>
                                }
                            }).collect_view()}
                            <div class="inspection-drawer__actions">
                                <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>"Enregistrer"</button>
                                {has_url_field.then(|| view! {
                                    <button type="button" class="btn btn-secondary"
                                        disabled=move || test_conn.pending().get()
                                        on:click=move |_| {
                                            test_conn.dispatch(settings_body(&form.read_untracked().entries()));
                                        }>
                                        {move || if test_conn.pending().get() { "Test…" } else { "Tester la connexion" }}
                                    </button>
                                })}
                            </div>
                        </form>
                    }.into_any()
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_is_sent_and_an_empty_secret_stays_empty() {
        let field = |name: &str, kind| ResolvedSettingsField {
            name: name.into(),
            label: name.into(),
            field_type: kind,
            required: false,
            description: None,
            value: String::new(),
            is_set: true,
        };
        let body = settings_body(&[
            (
                field("url", SettingsFieldType::Url),
                "http://sonarr:8989".into(),
            ),
            (field("apiKey", SettingsFieldType::Secret), String::new()),
        ]);
        assert_eq!(body["url"], "http://sonarr:8989");
        assert_eq!(body["apiKey"], "");
    }
}
