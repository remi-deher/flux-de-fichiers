//! Onglet « Journal » de l'écran Activité : lignes récentes du journal applicatif du central.

use crate::api;
use crate::ui::{Icon, IconName, SectionHeader};
use forgearr_shared::logs::{DEFAULT_LOGS_LIMIT, LogEntry, LogLevel, LogsResponse};
use leptos::ev;
use leptos::prelude::*;

const LOGS_PATH: &str = "/api/v1/logs";
const TIMESTAMP_CHARS: usize = 19;

/// Encode une valeur de paramètre d'URL (seuls les caractères non réservés restent tels quels).
fn encode_component(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub fn build_logs_path(level: Option<LogLevel>, search: &str) -> String {
    let mut path = format!("{LOGS_PATH}?limit={DEFAULT_LOGS_LIMIT}");
    if let Some(level) = level {
        path.push_str(&format!("&level={}", level.as_str()));
    }
    let search = search.trim();
    if !search.is_empty() {
        path.push_str(&format!("&search={}", encode_component(search)));
    }
    path
}

fn level_pill_class(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "status-pill danger",
        LogLevel::Warn => "status-pill warning",
        LogLevel::Info => "status-pill success",
        LogLevel::Debug | LogLevel::Trace => "status-pill",
    }
}

/// `2026-11-18T17:10:06.123Z` devient `2026-11-18 17:10:06` (heure UTC du central).
fn short_timestamp(raw: &str) -> String {
    raw.chars()
        .take(TIMESTAMP_CHARS)
        .map(|c| if c == 'T' { ' ' } else { c })
        .collect()
}

#[component]
fn LogRow(entry: LogEntry) -> impl IntoView {
    let pill = level_pill_class(entry.level);
    view! {
        <tr>
            <td class="text-dim text-mono">{short_timestamp(&entry.timestamp)}</td>
            <td><span class=pill>{entry.level.label()}</span></td>
            <td><code>{entry.target}</code></td>
            <td>
                <span>{entry.message}</span>
                {entry.fields.into_iter().map(|field| view! {
                    <div class="form-help text-mono">{format!("{} = {}", field.name, field.value)}</div>
                }).collect_view()}
            </td>
        </tr>
    }
}

#[component]
pub fn LogsPanel() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let level = RwSignal::new(None::<LogLevel>);
    let search_input = RwSignal::new(String::new());
    let search = RwSignal::new(String::new());
    let logs = LocalResource::new(move || {
        reload.track();
        let path = build_logs_path(level.get(), &search.get());
        async move { api::get::<LogsResponse>(&path).await }
    });

    let submit = move |event: ev::SubmitEvent| {
        event.prevent_default();
        search.set(search_input.get_untracked());
    };

    let actions = view! {
        <form class="cluster" on:submit=submit>
            <select
                class="form-select"
                aria-label="Niveau minimal"
                on:change=move |event| level.set(LogLevel::parse(&event_target_value(&event)))
            >
                <option value="">"Tous les niveaux"</option>
                {LogLevel::ALL.into_iter().map(|item| view! {
                    <option value=item.as_str()>{format!("{} et plus", item.label())}</option>
                }).collect_view()}
            </select>
            <input
                class="form-input"
                type="search"
                aria-label="Rechercher dans le journal"
                placeholder="Rechercher (message, module, champ)…"
                prop:value=move || search_input.get()
                on:input=move |event| search_input.set(event_target_value(&event))
            />
            <button type="submit" class="btn btn-secondary btn-sm">"Rechercher"</button>
            <button
                type="button"
                class="btn btn-secondary btn-sm"
                on:click=move |_| reload.update(|n| *n = n.wrapping_add(1))
            >
                <Icon name=IconName::Refresh size=14 />
                <span>"Actualiser"</span>
            </button>
        </form>
    }
    .into_any();

    view! {
        <section class="section-panel">
            <SectionHeader
                title="Journal du central"
                subtitle="Événements récents de l'application (erreurs, alertes, appels sortants). Heure UTC, plus récent en premier."
                actions=actions
            />
            <div class="registre">
                {move || match logs.get() {
                    None => view! { <p class="form-help">"Chargement du journal…"</p> }.into_any(),
                    Some(Err(err)) => view! {
                        <p class="form-error" role="alert">{err.message()}</p>
                    }.into_any(),
                    Some(Ok(response)) if response.entries.is_empty() => view! {
                        <p class="form-help">"Aucune ligne pour ce filtre."</p>
                    }.into_any(),
                    Some(Ok(response)) => {
                        let count = response.entries.len();
                        let note = response.truncated.then_some(
                            "Lecture bornée : affinez le filtre pour remonter plus loin.",
                        );
                        view! {
                            <div class="stack-xs">
                                <p class="form-help">
                                    {format!("{count} ligne(s) affichée(s). ")}
                                    {note}
                                </p>
                                <div class="table-wrapper">
                                    <table class="data-table" aria-label="Journal du central">
                                        <thead>
                                            <tr>
                                                <th scope="col">"Heure"</th>
                                                <th scope="col">"Niveau"</th>
                                                <th scope="col">"Module"</th>
                                                <th scope="col">"Message"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {response.entries.into_iter().map(|entry| view! {
                                                <LogRow entry=entry />
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_path_encodes_search_and_level() {
        assert_eq!(build_logs_path(None, "  "), "/api/v1/logs?limit=200");
        assert_eq!(
            build_logs_path(Some(LogLevel::Warn), "échec LLM&x=1"),
            "/api/v1/logs?limit=200&level=warn&search=%C3%A9chec%20LLM%26x%3D1"
        );
    }

    #[test]
    fn timestamps_and_pills_are_compact() {
        assert_eq!(
            short_timestamp("2026-11-18T17:10:06.123456Z"),
            "2026-11-18 17:10:06"
        );
        assert_eq!(level_pill_class(LogLevel::Error), "status-pill danger");
        assert_eq!(level_pill_class(LogLevel::Trace), "status-pill");
    }
}
