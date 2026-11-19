//! Historique des appels et tâches effectuées via le serveur MCP (ADR 0015).

use crate::api;
use crate::format;
use crate::ui::search::{matches, query};
use crate::ui::{Icon, IconName, SectionHeader};
use forgearr_shared::mcp::{McpAuditListResponse, McpAuditOutcome};
use leptos::prelude::*;

const MCP_AUDIT_PATH: &str = "/api/v1/mcp/audit";
const AUDIT_FILTER_OUTCOMES: [(&str, &str); 5] = [
    ("", "Tous les statuts"),
    ("ok", "Succès"),
    ("error", "Erreur"),
    ("forbidden", "Refusé"),
    ("rate_limited", "Limité"),
];

pub fn outcome_pill_class(outcome: McpAuditOutcome) -> &'static str {
    match outcome {
        McpAuditOutcome::Ok => "status-pill success",
        McpAuditOutcome::Error => "status-pill danger",
        McpAuditOutcome::Forbidden | McpAuditOutcome::RateLimited => "status-pill warning",
    }
}

pub fn build_audit_path(outcome_filter: &str) -> String {
    let trimmed = outcome_filter.trim();
    if trimmed.is_empty() {
        format!("{MCP_AUDIT_PATH}?limit=50")
    } else {
        format!("{MCP_AUDIT_PATH}?limit=50&outcome={trimmed}")
    }
}

#[component]
pub fn McpAuditSection() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let outcome_filter = RwSignal::new(String::new());
    let audit = LocalResource::new(move || {
        reload.track();
        let path = build_audit_path(&outcome_filter.get());
        async move { api::get::<McpAuditListResponse>(&path).await }
    });

    let header_actions = view! {
        <div class="cluster">
            <select
                class="form-select"
                aria-label="Filtrer par statut MCP"
                on:change=move |event| outcome_filter.set(event_target_value(&event))
            >
                {AUDIT_FILTER_OUTCOMES.into_iter().map(|(val, label)| view! {
                    <option value=val>{label}</option>
                }).collect_view()}
            </select>
            <button
                type="button"
                class="btn btn-secondary btn-sm"
                on:click=move |_| reload.update(|n| *n = n.wrapping_add(1))
            >
                <Icon name=IconName::Refresh size=14 />
                <span>"Actualiser"</span>
            </button>
        </div>
    }
    .into_any();

    let visible_entries = Signal::derive(move || {
        let text = query();
        let data = audit.get().and_then(Result::ok);
        let total = data.as_ref().map_or(0, |resp| resp.total);
        let entries = data.map_or_else(Vec::new, |resp| resp.entries);
        let section_hit = !text.trim().is_empty()
            && matches(&["mcp", "audit", "historique", "ia", "journal"], &text);
        let filtered = if text.trim().is_empty() || section_hit {
            entries
        } else {
            entries
                .into_iter()
                .filter(|entry| {
                    let target = entry.target_summary.as_deref().unwrap_or("");
                    matches(
                        &[
                            &entry.key_name,
                            &entry.tool_name,
                            target,
                            entry.outcome.label(),
                        ],
                        &text,
                    )
                })
                .collect()
        };
        (total, filtered)
    });

    view! {
        <section class="section-panel settings-section">
            <SectionHeader
                title="Historique des actions MCP"
                subtitle="Journal des appels d'outils et lectures MCP par clé d'API (aucun secret conservé)."
                actions=header_actions
            />
            <div class="registre">
                {move || match audit.get() {
                    None => view! { <p class="form-help">"Chargement du journal MCP…"</p> }.into_any(),
                    Some(Err(err)) => view! {
                        <p class="form-help" role="status">{err.message()}</p>
                    }.into_any(),
                    Some(Ok(_)) => {
                        let (total, rows) = visible_entries.get();
                        if rows.is_empty() {
                            view! {
                                <p class="form-help">"Aucune action MCP enregistrée pour ce filtre."</p>
                            }.into_any()
                        } else {
                            view! {
                                <div class="stack-xs">
                                    <p class="form-help">{format!("{total} événement(s) enregistré(s)")}</p>
                                    <div class="table-wrapper">
                                        <table class="data-table" aria-label="Historique des actions MCP">
                                            <thead>
                                                <tr>
                                                    <th scope="col">"Date"</th>
                                                    <th scope="col">"Clé / Acteur"</th>
                                                    <th scope="col">"Outil"</th>
                                                    <th scope="col">"Cible"</th>
                                                    <th scope="col">"Résultat"</th>
                                                    <th scope="col" class="cell-end">"Durée"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {rows.into_iter().map(|entry| {
                                                    let when = format::datetime(entry.created_at);
                                                    let target = entry
                                                        .target_summary
                                                        .clone()
                                                        .unwrap_or_else(|| "—".to_owned());
                                                    let pill = outcome_pill_class(entry.outcome);
                                                    let outcome_label = entry.outcome.label();
                                                    let duration = format!("{} ms", entry.duration_ms);
                                                    let err_detail = entry.error_message.clone();
                                                    view! {
                                                        <tr>
                                                            <td class="text-dim">{when}</td>
                                                            <td class="text-strong">{entry.key_name}</td>
                                                            <td><code>{entry.tool_name}</code></td>
                                                            <td class="text-dim">
                                                                <span>{target}</span>
                                                                {err_detail.map(|msg| view! {
                                                                    <div class="form-help">{msg}</div>
                                                                })}
                                                            </td>
                                                            <td><span class=pill>{outcome_label}</span></td>
                                                            <td class="cell-end text-dim">{duration}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>
                                </div>
                            }.into_any()
                        }
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
    fn build_audit_path_handles_optional_outcome_filter() {
        assert_eq!(build_audit_path(""), "/api/v1/mcp/audit?limit=50");
        assert_eq!(
            build_audit_path("forbidden"),
            "/api/v1/mcp/audit?limit=50&outcome=forbidden"
        );
        assert_eq!(
            outcome_pill_class(McpAuditOutcome::RateLimited),
            "status-pill warning"
        );
    }
}
