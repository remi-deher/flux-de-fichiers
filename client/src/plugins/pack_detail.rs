//! Fiche d'inspection à onglets d'un pack : composants, installation et informations.

use super::{date_row, section_badge_label};
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Icon, IconName, Tag, TagTone, Toasts};
use flux_shared::plugins::{PackDetailView, PackInstallResult};
use leptos::prelude::*;

const INSTALL_ICON_SIZE: u32 = 15;

/// Onglets du tiroir d'un pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackDrawerTab {
    Components,
    Install,
    Info,
}

impl PackDrawerTab {
    pub const TABS: &'static [&'static str] = &["Composants", "Installation", "Informations"];

    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Components,
            1 => Self::Install,
            _ => Self::Info,
        }
    }
}

/// Synthèse textuelle du bilan d'installation retourné par `POST /api/v1/packs/{id}/install`.
pub fn pack_install_summary(result: &PackInstallResult) -> (bool, String) {
    let ok = result.failed_tools.is_empty() && result.missing_components.is_empty();
    let msg = if ok {
        format!(
            "{} outil(s) installé(s), {} composant(s) vérifié(s).",
            result.installed_tools.len(),
            result.verified_components.len()
        )
    } else {
        format!(
            "{} outil(s) en échec, {} composant(s) manquant(s).",
            result.failed_tools.len(),
            result.missing_components.len()
        )
    };
    (ok, msg)
}

#[component]
fn ComponentsTab(detail: PackDetailView) -> impl IntoView {
    let spec = detail.installation;
    view! {
        {spec.entrypoint.map(|entry| view! {
            <div class="detail-grid">
                <div class="detail-term">"Point d'entrée"</div>
                <div class="detail-val">
                    <Tag label=section_badge_label(entry.kind) tone=TagTone::Primary />
                    " "
                    <span class="text-mono text-strong">{entry.id}</span>
                    " "
                    <Tag label=entry.version tone=TagTone::Mono />
                </div>
            </div>
        })}
        <div class="pack-components">
            <h3 class="form-label">"Composants déclarés"</h3>
            {if spec.components.is_empty() {
                view! { <p class="text-dim text-sm">"Aucun composant déclaré."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {spec.components.into_iter().map(|comp| view! {
                            <li class="step-item">
                                <div class="step-item__head">
                                    <Tag label=section_badge_label(comp.kind) tone=TagTone::Primary />
                                    <span class="text-mono text-strong">{comp.id}</span>
                                    <Tag label=comp.version tone=TagTone::Mono />
                                </div>
                            </li>
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn InstallTab(
    detail: PackDetailView,
    can_install: bool,
    install: Action<(), Result<PackInstallResult, api::ApiError>>,
    last_result: RwSignal<Option<PackInstallResult>>,
    feedback: RwSignal<Option<String>>,
) -> impl IntoView {
    let spec = detail.installation;
    let runtime_label = match spec.runtime_implemented {
        Some(true) => "Prêt",
        Some(false) => "Non implémenté",
        None => "Déclaratif",
    };
    view! {
        <div class="detail-grid">
            <div class="detail-term">"Exécution"</div>
            <div class="detail-val">{runtime_label}</div>
            <div class="detail-term">"Composants"</div>
            <div class="detail-val">{spec.components.len().to_string()}</div>
        </div>
        {can_install.then(|| view! {
            <div class="inspection-drawer__actions">
                <button
                    type="button"
                    class="btn btn-primary"
                    disabled=move || install.pending().get()
                    on:click=move |_| { install.dispatch(()); }
                >
                    <Icon name=IconName::Download size=INSTALL_ICON_SIZE />
                    <span>{move || if install.pending().get() { "Installation…" } else { "Installer le pack" }}</span>
                </button>
            </div>
        })}
        {move || last_result.get().map(|res| view! {
            <div class="pack-components">
                <h3 class="form-label">"Bilan de l'installation"</h3>
                <div class="cluster">
                    <Tag label=format!("Outils installés : {}", res.installed_tools.len()) tone=TagTone::Success />
                    <Tag label=format!("Composants vérifiés : {}", res.verified_components.len()) tone=TagTone::Primary />
                    {(!res.failed_tools.is_empty()).then(|| view! {
                        <Tag label=format!("Échecs outils : {}", res.failed_tools.len()) tone=TagTone::Danger />
                    })}
                    {(!res.missing_components.is_empty()).then(|| view! {
                        <Tag label=format!("Manquants : {}", res.missing_components.len()) tone=TagTone::Warning />
                    })}
                </div>
                {(!res.failed_tools.is_empty()).then(|| view! {
                    <ul class="step-list">
                        {res.failed_tools.into_iter().map(|(tool_id, reason)| view! {
                            <li class="step-item">
                                <span class="text-mono text-strong">{tool_id}</span>
                                <p class="form-error text-sm">{reason}</p>
                            </li>
                        }).collect_view()}
                    </ul>
                })}
            </div>
        })}
        <p class="form-help" role="status">{move || feedback.get()}</p>
    }
}

#[component]
fn InfoTab(detail: PackDetailView) -> impl IntoView {
    view! {
        {detail.description.map(|desc| view! {
            <p class="text-sm">{desc}</p>
        })}
        <div class="detail-grid">
            <div class="detail-term">"Identifiant"</div>
            <div class="detail-val text-mono">{detail.id}</div>
            <div class="detail-term">"Nom"</div>
            <div class="detail-val">{detail.name}</div>
            <div class="detail-term">"Version"</div>
            <div class="detail-val text-mono">{detail.version}</div>
            {date_row("Mise à jour", detail.updated_at)}
            <div class="detail-term">"Format"</div>
            <div class="detail-val text-mono">{detail.installation.kind}</div>
            {detail.category.filter(|cat| cat.shows_badge()).map(|cat| view! {
                <div class="detail-term">"Catégorie"</div>
                <div class="detail-val"><Tag label=cat.as_str() tone=TagTone::Neutral /></div>
            })}
            {detail.author.map(|author| view! {
                <div class="detail-term">"Auteur"</div>
                <div class="detail-val">{author}</div>
            })}
            {detail.license.map(|license| view! {
                <div class="detail-term">"Licence"</div>
                <div class="detail-val text-mono">{license}</div>
            })}
            {detail.compatibility.app_min.map(|min| view! {
                <div class="detail-term">"App minimale"</div>
                <div class="detail-val text-mono">{min}</div>
            })}
            {(!detail.compatibility.platforms.is_empty()).then(|| view! {
                <div class="detail-term">"Plateformes"</div>
                <div class="detail-val">
                    <div class="cluster">
                        {detail.compatibility.platforms.into_iter().map(|plat| view! {
                            <Tag label=plat tone=TagTone::Mono />
                        }).collect_view()}
                    </div>
                </div>
            })}
            {(!detail.tags.is_empty()).then(|| view! {
                <div class="detail-term">"Étiquettes"</div>
                <div class="detail-val">
                    <div class="cluster">
                        {detail.tags.into_iter().map(|tag| view! {
                            <Tag label=tag tone=TagTone::Neutral />
                        }).collect_view()}
                    </div>
                </div>
            })}
        </div>
    }
}

#[component]
pub fn PackDetail(
    pack_id: String,
    tab: RwSignal<usize>,
    can_install: bool,
    on_installed: Callback<()>,
) -> impl IntoView {
    let encoded = encode_segment(&pack_id);
    let spec_path = format!("/api/v1/packs/{encoded}");
    let install_path = format!("/api/v1/packs/{encoded}/install");

    let detail = LocalResource::new(move || {
        let path = spec_path.clone();
        async move { api::get::<PackDetailView>(&path).await }
    });
    let feedback = RwSignal::new(None::<String>);
    let last_result = RwSignal::new(None::<PackInstallResult>);

    let install = Action::new_local(move |(): &()| {
        let path = install_path.clone();
        async move { api::post::<(), PackInstallResult>(&path, None).await }
    });

    Effect::new(move |_| match install.value().get() {
        Some(Ok(result)) => {
            let (ok, summary) = pack_install_summary(&result);
            last_result.set(Some(result));
            if ok {
                Toasts::set_success(feedback, summary);
            } else {
                Toasts::set_error(feedback, summary);
            }
            on_installed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = PackDrawerTab::from_index(tab.get());
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement du pack…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => match active_tab {
                        PackDrawerTab::Components => view! { <ComponentsTab detail=data /> }.into_any(),
                        PackDrawerTab::Install => view! {
                            <InstallTab
                                detail=data
                                can_install=can_install
                                install=install
                                last_result=last_result
                                feedback=feedback
                            />
                        }
                        .into_any(),
                        PackDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
                    },
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_drawer_tabs_map_all_indices() {
        assert_eq!(
            PackDrawerTab::TABS,
            &["Composants", "Installation", "Informations"]
        );
        assert_eq!(PackDrawerTab::from_index(0), PackDrawerTab::Components);
        assert_eq!(PackDrawerTab::from_index(1), PackDrawerTab::Install);
        assert_eq!(PackDrawerTab::from_index(2), PackDrawerTab::Info);
        assert_eq!(PackDrawerTab::from_index(10), PackDrawerTab::Info);
    }

    #[test]
    fn pack_install_summary_reports_success_and_failures() {
        let ok = PackInstallResult {
            pack_id: "media".into(),
            pack_version: "1.0.0".into(),
            installed_tools: vec!["ffmpeg".into(), "mp4box".into()],
            failed_tools: Vec::new(),
            verified_components: vec!["video".into()],
            missing_components: Vec::new(),
        };
        let (passed, summary) = pack_install_summary(&ok);
        assert!(passed);
        assert!(summary.contains("2 outil(s) installé(s)"));
        assert!(summary.contains("1 composant(s) vérifié(s)"));

        let partial = PackInstallResult {
            failed_tools: vec![("ffmpeg".into(), "réseau indisponible".into())],
            missing_components: vec!["audio".into()],
            ..ok
        };
        let (passed, summary) = pack_install_summary(&partial);
        assert!(!passed);
        assert!(summary.contains("1 outil(s) en échec"));
        assert!(summary.contains("1 composant(s) manquant(s)"));
    }
}
