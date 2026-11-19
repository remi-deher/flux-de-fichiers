//! Réglages d'un plugin, générés depuis son `settingsSchema` (secrets jamais relus).

use crate::api;
use crate::route::encode_segment;
use forgearr_shared::plugins::{
    PluginConnectionTestResult, PluginSettingsView, ResolvedSettingsField, SettingsFieldType,
};
use leptos::prelude::*;
use serde_json::{Map, Value};

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
    let load_path = path.clone();
    let view_data = LocalResource::new(move || {
        let path = load_path.clone();
        async move { api::get::<PluginSettingsView>(&path).await }
    });
    let test_path = format!("/api/v1/plugins/{}/test", encode_segment(&plugin_id));
    let feedback = RwSignal::new(None::<String>);
    let test_feedback = RwSignal::new(None::<(bool, String)>);
    let save = Action::new_local(move |body: &Map<String, Value>| {
        let (path, body) = (path.clone(), body.clone());
        async move { api::put::<Map<String, Value>, Value>(&path, &body).await }
    });
    let test_conn = Action::new_local(move |body: &Map<String, Value>| {
        let (path, body) = (test_path.clone(), body.clone());
        async move {
            api::post::<Map<String, Value>, PluginConnectionTestResult>(&path, Some(&body)).await
        }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok(_)) => feedback.set(Some("Réglages enregistrés.".into())),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    Effect::new(move |_| match test_conn.value().get() {
        Some(Ok(res)) => test_feedback.set(Some((res.ok, res.message))),
        Some(Err(error)) => test_feedback.set(Some((false, error.message()))),
        None => {}
    });
    view! {
        <div class="inspection-panel">
            {move || match view_data.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(data)) if data.fields.is_empty() => view! { <p class="text-dim">"Ce plugin n'a pas de réglage."</p> }.into_any(),
                Some(Ok(data)) => {
                    let has_url_field = data
                        .fields
                        .iter()
                        .any(|field| field.field_type == SettingsFieldType::Url);
                    let values: Vec<(ResolvedSettingsField, RwSignal<String>)> = data
                        .fields
                        .into_iter()
                        .map(|field| { let value = RwSignal::new(field.value.clone()); (field, value) })
                        .collect();
                    let stored = StoredValue::new(values.clone());
                    let stored_for_test = stored;
                    view! {
                        <form on:submit=move |event| {
                            event.prevent_default();
                            let current: Vec<(ResolvedSettingsField, String)> = stored
                                .read_value()
                                .iter()
                                .map(|(field, value)| (field.clone(), value.get_untracked()))
                                .collect();
                            save.dispatch(settings_body(&current));
                        }>
                            {values.into_iter().map(|(field, value)| {
                                let id = format!("plugin-setting-{}", field.name);
                                let placeholder = if field.is_set { "Défini — laisser vide pour conserver" } else { "" };
                                let label = if field.required { format!("{} *", field.label) } else { field.label.clone() };
                                view! {
                                    <div class="form-group">
                                        <label for=id.clone() class="form-label">{label}</label>
                                        <input id=id class="form-input" type=input_kind(&field) autocomplete="off" placeholder=placeholder
                                            prop:value=move || value.get()
                                            on:input=move |event| value.set(event_target_value(&event)) />
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
                                            let current: Vec<(ResolvedSettingsField, String)> = stored_for_test
                                                .read_value()
                                                .iter()
                                                .map(|(field, value)| (field.clone(), value.get_untracked()))
                                                .collect();
                                            test_conn.dispatch(settings_body(&current));
                                        }>
                                        {move || if test_conn.pending().get() { "Test…" } else { "Tester la connexion" }}
                                    </button>
                                })}
                            </div>
                        </form>
                    }.into_any()
                }
            }}
            <p class="form-help" role="status">{move || feedback.get()}</p>
            {move || test_feedback.get().map(|(ok, msg)| view! {
                <p class=if ok { "form-help" } else { "form-error" } role="status">{msg}</p>
            })}
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
