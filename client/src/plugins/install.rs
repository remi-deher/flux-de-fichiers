//! Assistant d'installation (ADR 0029 §4) : plan, accords, outils à télécharger, puis application.

use super::store::InstallTarget;
use super::{STORE_TOPICS, section_badge_label};
use crate::api;
use crate::live::use_live;
use crate::ui::{Tag, TagTone, Toasts};
use forgearr_shared::plugins::store::plan::{
    InstallOutcome, InstallPlan, InstallRequest, PlanAction, PlanRequest,
};
use leptos::prelude::*;
use std::collections::BTreeSet;

const PLAN_PATH: &str = "/api/v1/installations/plan";
const APPLY_PATH: &str = "/api/v1/installations/apply";

pub fn action_label(action: PlanAction) -> (&'static str, TagTone) {
    match action {
        PlanAction::Install => ("Installé", TagTone::Primary),
        PlanAction::Update => ("Mis à jour", TagTone::Warning),
        PlanAction::Repair => ("Réparé", TagTone::Danger),
        PlanAction::AlreadyInstalled => ("Déjà présent", TagTone::Neutral),
    }
}

/// Le plan peut-il être appliqué : rien ne le bloque, tous ses accords sont donnés, et il change quelque chose ?
pub fn can_apply(plan: &InstallPlan, accepted: usize) -> bool {
    plan.blockers.is_empty()
        && accepted == plan.consents.len()
        && plan
            .items
            .iter()
            .any(|item| item.action != PlanAction::AlreadyInstalled)
}

/// Bilan d'une installation : réussite, et message.
pub fn outcome_summary(outcome: &InstallOutcome) -> (bool, String) {
    let mut message = format!("{} élément(s) installé(s)", outcome.installed.len());
    if !outcome.installed_tools.is_empty() {
        message.push_str(&format!(
            ", {} outil(s) téléchargé(s)",
            outcome.installed_tools.len()
        ));
    }
    if outcome.failed_tools.is_empty() {
        message.push('.');
        return (true, message);
    }
    let failed: Vec<&str> = outcome
        .failed_tools
        .iter()
        .map(|tool| tool.id.as_str())
        .collect();
    message.push_str(&format!(
        ". Outil(s) à télécharger plus tard : {}.",
        failed.join(", ")
    ));
    (false, message)
}

#[component]
pub fn InstallWizard(target: InstallTarget, open: RwSignal<bool>) -> impl IntoView {
    let live = use_live();
    let targets = vec![target.item];
    let request = PlanRequest {
        targets: targets.clone(),
    };
    let plan = LocalResource::new(move || {
        let request = request.clone();
        async move { api::post::<_, InstallPlan>(PLAN_PATH, Some(&request)).await }
    });
    let accepted = RwSignal::new(BTreeSet::<usize>::new());
    let tools = RwSignal::new(None::<BTreeSet<String>>);
    let apply = Action::new_local(move |request: &InstallRequest| {
        let request = request.clone();
        async move { api::post::<_, InstallOutcome>(APPLY_PATH, Some(&request)).await }
    });
    Effect::new(move |_| match apply.value().get() {
        Some(Ok(outcome)) => {
            let (ok, message) = outcome_summary(&outcome);
            if ok {
                Toasts::notify_success(message);
            } else {
                Toasts::notify_error(message);
            }
            live.touch(STORE_TOPICS);
            open.set(false);
        }
        Some(Err(error)) => Toasts::notify_error(error.message()),
        None => {}
    });

    view! {
        {move || match plan.get() {
            None => view! { <p class="text-dim">"Calcul du plan…"</p> }.into_any(),
            Some(Err(error)) => view! { <p class="form-error">{error.message()}</p> }.into_any(),
            Some(Ok(plan)) => {
                if tools.read_untracked().is_none() {
                    tools.set(Some(
                        plan.tools.iter().filter(|tool| tool.preselected).map(|tool| tool.id.clone()).collect(),
                    ));
                }
                plan_view(plan, targets.clone(), accepted, tools, apply).into_any()
            }
        }}
    }
}

fn plan_view(
    plan: InstallPlan,
    targets: Vec<forgearr_shared::plugins::store::ItemRef>,
    accepted: RwSignal<BTreeSet<usize>>,
    tools: RwSignal<Option<BTreeSet<String>>>,
    apply: Action<InstallRequest, Result<InstallOutcome, api::ApiError>>,
) -> impl IntoView {
    let consents_count = plan.consents.len();
    let blocked = !plan.blockers.is_empty();
    let ready = {
        let plan = plan.clone();
        move || can_apply(&plan, accepted.read().len()) && !apply.pending().get()
    };
    let submit = {
        let plan = plan.clone();
        move |_| {
            apply.dispatch(InstallRequest {
                targets: targets.clone(),
                plan_id: plan.plan_id.clone(),
                consents: plan.consents.clone(),
                tools: tools.get().unwrap_or_default().into_iter().collect(),
            });
        }
    };
    view! {
        <div class="install-plan">
            {blocked.then(|| view! {
                <section class="install-plan__section" data-tone="danger">
                    <h3>"Installation impossible"</h3>
                    <ul>
                        {plan.blockers.iter().map(|blocker| view! { <li>{blocker.message()}</li> }).collect_view()}
                    </ul>
                </section>
            })}
            <section class="install-plan__section">
                <h3>"Éléments"</h3>
                <ul class="install-plan__list">
                    {plan.items.iter().map(|item| {
                        let (label, tone) = action_label(item.action);
                        view! {
                            <li>
                                <span>{item.name.clone()}</span>
                                <Tag label=section_badge_label(item.kind) tone=TagTone::Mono />
                                <Tag label=format!("v{}", item.version) tone=TagTone::Neutral />
                                <Tag label=label tone=tone />
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </section>
            {(!plan.tools.is_empty()).then(|| view! {
                <section class="install-plan__section">
                    <h3>"Outils à télécharger"</h3>
                    <p class="text-dim">"Un outil décoché sera téléchargé à son premier usage."</p>
                    {plan.tools.iter().map(|tool| {
                        let id = tool.id.clone();
                        let checked_id = id.clone();
                        view! {
                            <label class="form-check">
                                <input
                                    type="checkbox"
                                    disabled=!tool.installable
                                    prop:checked=move || tools.read().as_ref().is_some_and(|set| set.contains(&checked_id))
                                    on:change=move |_| tools.update(|set| {
                                        let set = set.get_or_insert_with(BTreeSet::new);
                                        if !set.remove(&id) {
                                            set.insert(id.clone());
                                        }
                                    })
                                />
                                <span>{format!("{} v{}", tool.name, tool.version)}</span>
                            </label>
                        }
                    }).collect_view()}
                </section>
            })}
            {(consents_count > 0).then(|| view! {
                <section class="install-plan__section" data-tone="warning">
                    <h3>"À accepter"</h3>
                    {plan.consents.iter().enumerate().map(|(index, consent)| view! {
                        <label class="form-check">
                            <input
                                type="checkbox"
                                prop:checked=move || accepted.read().contains(&index)
                                on:change=move |_| accepted.update(|set| {
                                    if !set.remove(&index) {
                                        set.insert(index);
                                    }
                                })
                            />
                            <span>{consent.message()}</span>
                        </label>
                    }).collect_view()}
                </section>
            })}
            {(!plan.warnings.is_empty()).then(|| view! {
                <ul class="install-plan__warnings text-dim">
                    {plan.warnings.iter().map(|warning| view! { <li>{warning.message()}</li> }).collect_view()}
                </ul>
            })}
            <div class="install-plan__actions">
                <button
                    type="button"
                    class="btn btn-primary"
                    disabled=move || !ready()
                    on:click=submit
                >
                    {move || if apply.pending().get() { "Installation…" } else { "Installer" }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::store::{
        ItemOrigin,
        plan::{PlanBlocker, PlanConsent, PlanItem, PlanReason, ToolFailure},
    };
    use forgearr_shared::plugins::vocab::CatalogItemType;

    fn item(action: PlanAction) -> PlanItem {
        PlanItem {
            origin: ItemOrigin {
                repository_id: "officiel".to_owned(),
                repository_url: "https://officiel.example/index.json".to_owned(),
                repository_name: "Officiel".to_owned(),
            },
            id: "video".to_owned(),
            kind: CatalogItemType::Plugin,
            name: "Vidéo".to_owned(),
            version: "1.0.0".to_owned(),
            action,
            reason: PlanReason::Target,
            required_by: Vec::new(),
            has_settings: false,
            configured: false,
        }
    }

    fn plan(action: PlanAction) -> InstallPlan {
        InstallPlan {
            plan_id: "p".to_owned(),
            items: vec![item(action)],
            tools: Vec::new(),
            blockers: Vec::new(),
            consents: Vec::new(),
            warnings: Vec::new(),
            suggested_repositories: Vec::new(),
        }
    }

    #[test]
    fn a_plan_applies_once_unblocked_consented_and_useful() {
        assert!(can_apply(&plan(PlanAction::Install), 0));
        assert!(!can_apply(&plan(PlanAction::AlreadyInstalled), 0));
        let mut consenting = plan(PlanAction::Install);
        consenting.consents.push(PlanConsent::DiscardSettings {
            id: "video".to_owned(),
        });
        assert!(!can_apply(&consenting, 0));
        assert!(can_apply(&consenting, 1));
        let mut blocked = plan(PlanAction::Install);
        blocked.blockers.push(PlanBlocker::ModifiedCopy {
            id: "video".to_owned(),
        });
        assert!(!can_apply(&blocked, 0));
    }

    #[test]
    fn the_summary_names_tools_left_for_later() {
        let mut outcome = InstallOutcome::default();
        assert_eq!(
            outcome_summary(&outcome),
            (true, "0 élément(s) installé(s).".to_owned())
        );
        outcome.failed_tools.push(ToolFailure {
            id: "ffmpeg".to_owned(),
            message: "réseau".to_owned(),
        });
        let (ok, message) = outcome_summary(&outcome);
        assert!(!ok);
        assert!(message.ends_with("Outil(s) à télécharger plus tard : ffmpeg."));
    }
}
