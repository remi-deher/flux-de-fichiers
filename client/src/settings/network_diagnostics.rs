//! Diagnostic : « Vérifier la configuration » rend des contrôles vert, orange ou rouge.

use crate::ui::{SectionHeader, StatusBadge};
use crate::{api, workers::origin};
use flux_shared::diagnostics::{
    CheckStatus, DiagnosticCheck, DiagnosticsReport, DiagnosticsRequest,
};
use leptos::prelude::*;

const DIAGNOSTICS_PATH: &str = "/api/v1/system/diagnostics";

/// Libellé et variante de pastille d'un verdict.
fn status_badge(status: CheckStatus) -> (&'static str, &'static str) {
    match status {
        CheckStatus::Ok => ("OK", "success"),
        CheckStatus::Warning => ("À vérifier", "warning"),
        CheckStatus::Error => ("Problème", "error"),
    }
}

#[component]
fn CheckRow(check: DiagnosticCheck) -> impl IntoView {
    let (label, variant) = status_badge(check.status);
    view! {
        <div class="inspection-panel">
            <p class="text-strong">
                {check.id.title()}" "<StatusBadge label=label variant=variant />
            </p>
            <p class="form-help">{check.explanation}</p>
            {check.fix.map(|fix| view! { <p class="form-help"><strong>"Correction : "</strong>{fix}</p> })}
        </div>
    }
}

#[component]
pub fn DiagnosticsCard() -> impl IntoView {
    let run = Action::new_local(|_: &()| async move {
        api::post::<_, DiagnosticsReport>(
            DIAGNOSTICS_PATH,
            Some(&DiagnosticsRequest {
                browser_origin: origin(),
            }),
        )
        .await
    });
    let actions = view! {
        <button type="button" class="btn btn-primary btn-sm" disabled=move || run.pending().get()
            on:click=move |_| { run.dispatch(()); }>"Vérifier la configuration"</button>
    }
    .into_any();
    view! {
        <div class="settings-section">
            <SectionHeader title="Diagnostic"
                subtitle="Cohérence de l'adresse publique, TLS, proxy, workers et courriel." actions=actions />
            {move || match run.value().get() {
                None => view! { <p class="text-dim">"Lancez la vérification pour contrôler cette installation."</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(report)) => report.checks.into_iter()
                    .map(|check| view! { <CheckRow check=check /> }).collect_view().into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_verdict_has_its_own_badge() {
        let variants: Vec<_> = [CheckStatus::Ok, CheckStatus::Warning, CheckStatus::Error]
            .into_iter()
            .map(|status| status_badge(status).1)
            .collect();
        assert_eq!(variants, ["success", "warning", "error"]);
    }
}
