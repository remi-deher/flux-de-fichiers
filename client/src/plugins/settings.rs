//! Réglages d'un plugin, générés depuis son `settingsSchema` (secrets jamais relus).

use crate::api;
use crate::route::encode_segment;
use flux_shared::plugins::{PluginSettingsView, ResolvedSettingsField, SettingsFieldType};
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
pub fn PluginSettings(plugin_id: String, on_close: Callback<()>) -> impl IntoView {
    let path = format!("/api/v1/plugins/{}/settings", encode_segment(&plugin_id));
    let load_path = path.clone();
    let view_data = LocalResource::new(move || {
        let path = load_path.clone();
        async move { api::get::<PluginSettingsView>(&path).await }
    });
    let feedback = RwSignal::new(None::<String>);
    let save = Action::new_local(move |body: &Map<String, Value>| {
        let (path, body) = (path.clone(), body.clone());
        async move { api::put::<Map<String, Value>, Value>(&path, &body).await }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok(_)) => feedback.set(Some("Réglages enregistrés.".into())),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    view! {
        <section class="section-panel" aria-labelledby="plugin-settings-title">
            <div class="cluster-spread">
                <h2 id="plugin-settings-title" class="section-title">{format!("Réglages — {plugin_id}")}</h2>
                <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| on_close.run(())>"Fermer"</button>
            </div>
            {move || match view_data.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(data)) if data.fields.is_empty() => view! { <p class="text-dim">"Ce plugin n'a pas de réglage."</p> }.into_any(),
                Some(Ok(data)) => {
                    let values: Vec<(ResolvedSettingsField, RwSignal<String>)> = data
                        .fields
                        .into_iter()
                        .map(|field| { let value = RwSignal::new(field.value.clone()); (field, value) })
                        .collect();
                    let stored = StoredValue::new(values.clone());
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
                            <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>"Enregistrer"</button>
                        </form>
                    }.into_any()
                }
            }}
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </section>
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
