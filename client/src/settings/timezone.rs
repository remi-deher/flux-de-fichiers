//! Horloge & Fuseau horaire : sélection IANA (heure d'été/hiver auto, UTC, GMT) et synchronisation navigateur.

use crate::api;
use crate::timezone::{TIMEZONE_PATH, browser_timezone, set_timezone};
use crate::ui::SectionHeader;
use chrono::Utc;
use forgearr_shared::system::SystemStatus;
use forgearr_shared::timezone::{
    DEFAULT_TIMEZONE, TIMEZONE_PRESETS, TimezoneGroup, UpdateTimezoneRequest, canonical_timezone,
    describe_instant,
};
use leptos::prelude::*;

const QUICK_ZONES: &[(&str, &str)] = &[
    ("Europe/Paris", "Europe/Paris (été/hiver)"),
    ("UTC", "UTC"),
    ("Etc/GMT", "GMT"),
    ("Etc/GMT-1", "UTC+01:00 fixe"),
];

#[component]
pub fn TimezoneControl(
    status: Signal<Option<Result<SystemStatus, String>>>,
    on_changed: Callback<()>,
) -> impl IntoView {
    let selected = RwSignal::new(crate::timezone::current_timezone());
    let custom_input = RwSignal::new(String::new());
    let initialized = RwSignal::new(false);
    let feedback = RwSignal::new(None::<Result<String, String>>);

    Effect::new(move || {
        if !initialized.get()
            && let Some(Ok(st)) = status.get()
        {
            selected.set(st.timezone.clone());
            custom_input.set(st.timezone);
            initialized.set(true);
        }
    });

    let save_action = Action::new_local(move |tz: &String| {
        let tz = tz.clone();
        async move {
            let Some(canonical) = canonical_timezone(&tz) else {
                return Err(
                    "Fuseau horaire IANA invalide (ex. Europe/Paris, UTC, Etc/GMT, America/Montreal)."
                        .to_owned(),
                );
            };
            let body = UpdateTimezoneRequest {
                timezone: canonical.clone(),
            };
            api::put::<_, SystemStatus>(TIMEZONE_PATH, &body)
                .await
                .map(|updated| updated.timezone)
                .map_err(|err| err.message())
        }
    });

    Effect::new(move || {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(saved_tz) => {
                    let _ = set_timezone(&saved_tz);
                    selected.set(saved_tz.clone());
                    custom_input.set(saved_tz.clone());
                    feedback.set(Some(Ok(format!("Fuseau horaire enregistré : {saved_tz}."))));
                    on_changed.run(());
                }
                Err(msg) => feedback.set(Some(Err(msg))),
            }
        }
    });

    let preview = Signal::derive(move || {
        let candidate = custom_input.get();
        let active = canonical_timezone(&candidate)
            .or_else(|| canonical_timezone(&selected.get()))
            .unwrap_or_else(|| DEFAULT_TIMEZONE.to_owned());
        describe_instant(Utc::now(), &active)
    });

    let apply_choice = move |tz: &str| {
        selected.set(tz.to_owned());
        custom_input.set(tz.to_owned());
        feedback.set(None);
    };

    let sync_browser = move |_| match browser_timezone() {
        Some(tz) => {
            selected.set(tz.clone());
            custom_input.set(tz.clone());
            save_action.dispatch(tz);
        }
        None => {
            feedback.set(Some(Err(
                "Impossible de détecter le fuseau horaire du navigateur.".to_owned(),
            )));
        }
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let raw = custom_input.get();
        let target = if raw.trim().is_empty() {
            selected.get()
        } else {
            raw
        };
        save_action.dispatch(target);
    };

    view! {
        <section class="settings-section section-panel">
            <SectionHeader
                title="Horloge & Fuseau horaire"
                subtitle="Définit l'heure de référence de l'interface, des journaux et des plannings workers (heure d'été/hiver automatique, UTC ou GMT)."
            />
            {move || {
                let info = preview.get();
                view! {
                    <div class="detail-grid">
                        <div class="detail-row">
                            <span class="detail-term">"Fuseau sélectionné"</span>
                            <span class="detail-val text-mono">{format!("{} ({})", info.timezone, info.utc_offset)}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-term">"Régime horaire"</span>
                            <span class="detail-val">{info.dst_state.label()}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-term">"Heure locale estimée"</span>
                            <span class="detail-val text-mono">{info.formatted_local}</span>
                        </div>
                        <div class="detail-row">
                            <span class="detail-term">"Référence UTC"</span>
                            <span class="detail-val text-mono">{info.formatted_utc}</span>
                        </div>
                    </div>
                }
            }}

            <form class="stack" on:submit=on_submit>
                <div class="form-row">
                    <label class="form-label" for="settings-timezone-select">"Fuseau horaire"</label>
                    <select
                        id="settings-timezone-select"
                        class="form-select"
                        prop:value=move || selected.get()
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            apply_choice(&value);
                        }
                    >
                        {TimezoneGroup::ALL.into_iter().map(|group| {
                            let label = group.label();
                            view! {
                                <optgroup label=label>
                                    {TIMEZONE_PRESETS.iter().filter(|p| p.group == group).map(|preset| view! {
                                        <option value=preset.id>{preset.label}</option>
                                    }).collect_view()}
                                </optgroup>
                            }
                        }).collect_view()}
                    </select>
                </div>

                <div class="form-row">
                    <label class="form-label" for="settings-timezone-custom">"Identifiant IANA (optionnel)"</label>
                    <input
                        id="settings-timezone-custom"
                        type="text"
                        class="form-input text-mono"
                        placeholder="Europe/Paris, UTC, Etc/GMT, America/Montreal…"
                        prop:value=move || custom_input.get()
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            custom_input.set(value.clone());
                            if let Some(canonical) = canonical_timezone(&value) {
                                selected.set(canonical);
                            }
                            feedback.set(None);
                        }
                    />
                    <span class="form-help">
                        "Les fuseaux régionaux (ex. Europe/Paris) basculent automatiquement entre heure d'hiver et heure d'été. UTC, GMT et Etc/GMT±N restent fixes toute l'année."
                    </span>
                </div>

                <div class="cluster cluster-wrap">
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        disabled=move || save_action.pending().get()
                        on:click=sync_browser
                    >
                        "Synchroniser avec le navigateur"
                    </button>
                    {QUICK_ZONES.iter().map(|(id, label)| {
                        let id = *id;
                        let label = *label;
                        view! {
                            <button
                                type="button"
                                class="btn btn-ghost btn-sm"
                                on:click=move |_| apply_choice(id)
                            >
                                {label}
                            </button>
                        }
                    }).collect_view()}
                </div>

                <div class="cluster cluster-wrap">
                    <button
                        type="submit"
                        class="btn btn-primary"
                        disabled=move || save_action.pending().get()
                    >
                        {move || if save_action.pending().get() { "Enregistrement…" } else { "Enregistrer le fuseau horaire" }}
                    </button>
                    {move || match feedback.get() {
                        Some(Ok(msg)) => view! { <span class="text-strong">{msg}</span> }.into_any(),
                        Some(Err(msg)) => view! { <span class="form-error">{msg}</span> }.into_any(),
                        None => ().into_any(),
                    }}
                </div>
            </form>
        </section>
    }
}
