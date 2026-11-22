//! Header commun : trois zones à largeur fixe, identiques sur tous les écrans.
//!
//! - gauche : retour éventuel, fil d'Ariane et titre de l'écran (le `h1` de la page) ;
//! - centre : recherche, dans une colonne de largeur constante (filtres et Ctrl K inclus) ;
//! - droite : une action principale, un menu « … » pour les autres, puis le statut et l'IA.
//!
//! La grille donne la même largeur aux colonnes latérales : ce que contient la zone de droite
//! (nombre d'actions, état des workers, mise à jour) ne déplace donc jamais la recherche.

use super::{NavDrawer, screen_group, screen_title, search_placeholder};
use crate::ai::AiAssistantState;
use crate::route::{Link, Route, use_router};
use crate::ui::filters::{FilterSheet, Filters};
use crate::ui::search::Search;
use crate::ui::topbar_action::{ActionSpec, TopbarAction};
use crate::ui::{CommandPaletteState, Icon, IconName};
use chrono::{DateTime, Utc};
use forgearr_shared::InstanceKind;
use forgearr_shared::session::Me;
use forgearr_shared::system::UpdateCheckStatus;
use forgearr_shared::workers::Worker;
use leptos::prelude::*;

/// Les libellés d'action qui commencent ainsi sont le retour de l'écran, pas son action principale.
const BACK_LABEL_PREFIX: &str = "Retour";
const HEADER_ICON_SIZE: u32 = 18;
const SEARCH_ICON_SIZE: u32 = 15;

/// Répartition des actions déclarées par un écran dans les emplacements du header.
#[derive(Default)]
struct ActionLayout {
    back: Option<ActionSpec>,
    primary: Option<ActionSpec>,
    more: Vec<ActionSpec>,
}

/// Premier retour → bouton de retour, première autre action → principale, le reste → menu « … ».
fn split_actions(specs: Vec<ActionSpec>) -> ActionLayout {
    let mut layout = ActionLayout::default();
    for spec in specs {
        if layout.back.is_none() && spec.label.starts_with(BACK_LABEL_PREFIX) {
            layout.back = Some(spec);
        } else if layout.primary.is_none() {
            layout.primary = Some(spec);
        } else {
            layout.more.push(spec);
        }
    }
    layout
}

/// Gravité résumée par le point de l'icône de statut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusTone {
    Ok,
    Warning,
    Danger,
}

impl StatusTone {
    const fn as_attr(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct UpdateNotice {
    label: String,
    title: String,
    url: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct StatusSummary {
    tone: StatusTone,
    workers: Option<String>,
    update: Option<UpdateNotice>,
}

fn workers_label(online: usize) -> String {
    match online {
        0 => "Aucun worker en ligne".to_owned(),
        1 => "1 worker en ligne".to_owned(),
        count => format!("{count} workers en ligne"),
    }
}

fn update_notice(info: &UpdateCheckStatus) -> UpdateNotice {
    let version = info.latest_version.clone().unwrap_or_default();
    UpdateNotice {
        label: format!("Mise à jour v{version}"),
        title: format!(
            "Nouvelle version v{version} disponible (installée : v{})",
            info.current_version
        ),
        url: info.release_url.clone(),
    }
}

/// Résume workers et mise à jour : aucun worker en ligne est critique, une mise à jour est un avertissement.
fn summarize_status(
    workers: Option<&[Worker]>,
    update: Option<&UpdateCheckStatus>,
    now: DateTime<Utc>,
) -> StatusSummary {
    let online = workers.map(|list| list.iter().filter(|worker| worker.is_online(now)).count());
    let update = update
        .filter(|info| info.enabled && info.update_available)
        .map(update_notice);
    let tone = if online == Some(0) {
        StatusTone::Danger
    } else if update.is_some() {
        StatusTone::Warning
    } else {
        StatusTone::Ok
    };
    StatusSummary {
        tone,
        workers: online.map(workers_label),
        update,
    }
}

/// Bouton-icône de taille fixe qui ouvre un panneau ; Échap ou un clic à côté le referme.
#[component]
fn HeaderMenu(
    label: &'static str,
    icon: IconName,
    #[prop(optional)] tone: Option<Signal<StatusTone>>,
    children: Children,
) -> impl IntoView {
    let open = RwSignal::new(false);
    crate::ui::listen_window(leptos::ev::keydown, move |event| {
        if event.key() == "Escape" {
            open.set(false);
        }
    });
    view! {
        <div class="header-menu">
            <button type="button" class="header-icon-btn" aria-label=label title=label
                aria-haspopup="menu" aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|value| *value = !*value)>
                <Icon name=icon size=HEADER_ICON_SIZE />
                {tone.map(|tone| view! {
                    <span class="header-icon-btn__dot" data-tone=move || tone.get().as_attr()></span>
                })}
            </button>
            <button type="button" class="header-menu__scrim" tabindex="-1" aria-label="Fermer le menu"
                hidden=move || !open.get() on:click=move |_| open.set(false)></button>
            <div class="header-menu__panel" role="menu" hidden=move || !open.get()
                on:click=move |_| open.set(false)>
                {children()}
            </div>
        </div>
    }
}

/// Icône de statut : workers en ligne et mise à jour disponible, détaillés dans son panneau.
#[component]
fn StatusMenu(
    workers: Signal<Option<Vec<Worker>>>,
    update: Signal<Option<UpdateCheckStatus>>,
) -> impl IntoView {
    let summary = Signal::derive(move || {
        workers.with(|workers| {
            update.with(|update| summarize_status(workers.as_deref(), update.as_ref(), Utc::now()))
        })
    });
    let tone = Signal::derive(move || summary.with(|summary| summary.tone));
    view! {
        <HeaderMenu label="Statut du système" icon=IconName::Activity tone=tone>
            {move || summary.with(|summary| {
                let workers_item = summary.workers.clone().map(|text| view! {
                    <Link to=Route::Workers class="header-menu__item">{text}</Link>
                });
                let update_item = summary.update.as_ref().map(|notice| {
                    let (label, title) = (notice.label.clone(), notice.title.clone());
                    match notice.url.clone() {
                        Some(url) => view! {
                            <a href=url target="_blank" rel="noopener noreferrer"
                                class="header-menu__item" title=title>{label}</a>
                        }.into_any(),
                        None => view! {
                            <Link to=Route::Settings class="header-menu__item">{label}</Link>
                        }.into_any(),
                    }
                });
                let nominal = workers_item.is_none() && update_item.is_none();
                view! {
                    {workers_item}
                    {update_item}
                    {nominal.then(|| view! { <span class="header-menu__note">"Aucun signalement"</span> })}
                }
            })}
        </HeaderMenu>
    }
}

#[component]
pub fn Topbar(
    me: Me,
    workers: Signal<Option<Vec<Worker>>>,
    update: Signal<Option<UpdateCheckStatus>>,
) -> impl IntoView {
    let router = use_router();
    let search = use_context::<Search>().unwrap_or_else(Search::install);
    let nav = use_context::<NavDrawer>().map(|drawer| drawer.0);
    let filters = use_context::<Filters>().unwrap_or_else(Filters::install);
    let palette = use_context::<CommandPaletteState>();
    let actions = use_context::<TopbarAction>().unwrap_or_else(TopbarAction::install);
    Effect::new(move |_| {
        router.current();
        search.text.set(String::new());
        filters.apply_route_change();
        if let Some(nav) = nav {
            nav.set(false);
        }
    });
    let central = me.instance_kind == InstanceKind::Central;
    let ai_state = central.then(use_context::<AiAssistantState>).flatten();
    view! {
        <header class="app-topbar" role="banner">
            <div class="topbar-container">
                <div class="topbar-lead">
                    <button type="button" class="topbar-menu-btn" aria-label="Ouvrir la navigation"
                        aria-expanded=move || nav.is_some_and(|open| open.get()).to_string()
                        on:click=move |_| if let Some(nav) = nav { nav.update(|open| *open = !*open) }>
                        <Icon name=IconName::Menu size=20 />
                    </button>
                    {move || split_actions(actions.specs()).back.map(|spec| view! {
                        <button type="button" class="header-icon-btn" aria-label=spec.label title=spec.label
                            on:click=move |_| spec.on_click.run(())>
                            <Icon name=IconName::ChevronLeft size=HEADER_ICON_SIZE />
                        </button>
                    })}
                    <div class="topbar-heading">
                        <span class="topbar-crumb">
                            {move || screen_group(&router.current()).unwrap_or_default()}
                        </span>
                        <h1 class="topbar-title">{move || screen_title(&router.current())}</h1>
                    </div>
                </div>
                <div class="topbar-search" role="search">
                    <Icon name=IconName::Search size=SEARCH_ICON_SIZE class="search-icon" />
                    <input type="search" class="topbar-search-input" autocomplete="off" spellcheck="false"
                        enterkeyhint="search" aria-label="Filtrer l'écran actif"
                        placeholder=move || search_placeholder(&router.current())
                        prop:value=move || search.text.get()
                        on:input=move |event| search.text.set(event_target_value(&event)) />
                    {move || (!search.text.get().is_empty()).then(|| view! {
                        <button type="button" class="topbar-search-clear" aria-label="Effacer le filtre"
                            title="Effacer le filtre" on:click=move |_| search.text.set(String::new())>
                            <Icon name=IconName::Close size=13 />
                        </button>
                    })}
                    <FilterSheet route=Signal::derive(move || router.current()) />
                    <button type="button" class="kbd-shortcut"
                        title="Ouvrir la palette de commandes (Ctrl+K)"
                        aria-label="Ouvrir la palette de commandes"
                        on:click=move |_| if let Some(palette) = palette { palette.show(); }>
                        "Ctrl K"
                    </button>
                </div>
                <div class="topbar-actions">
                    {move || {
                        let layout = split_actions(actions.specs());
                        let primary = layout.primary.map(|spec| view! {
                            <button type="button" class="btn btn-primary btn-sm"
                                on:click=move |_| spec.on_click.run(())>
                                <Icon name=IconName::Plus size=14 />
                                <span>{spec.label}</span>
                            </button>
                        });
                        let more = (!layout.more.is_empty()).then(|| view! {
                            <HeaderMenu label="Autres actions" icon=IconName::More>
                                {layout.more.into_iter().map(|spec| view! {
                                    <button type="button" role="menuitem" class="header-menu__item"
                                        on:click=move |_| spec.on_click.run(())>{spec.label}</button>
                                }).collect_view()}
                            </HeaderMenu>
                        });
                        view! { {primary} {more} }
                    }}
                    {central.then(|| view! {
                        <span class="topbar-actions__sep" aria-hidden="true"></span>
                        <StatusMenu workers=workers update=update />
                    })}
                    {ai_state.map(|ai| view! {
                        <button type="button" class="header-icon-btn ai-trigger-btn"
                            title="Ouvrir l'assistant IA intégré" aria-label="Assistant IA"
                            aria-expanded=move || ai.open.get().to_string()
                            on:click=move |_| ai.toggle()>
                            <Icon name=IconName::Sparkles size=HEADER_ICON_SIZE />
                        </button>
                    })}
                </div>
            </div>
        </header>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(label: &'static str) -> ActionSpec {
        ActionSpec {
            label,
            on_click: Callback::new(|()| {}),
        }
    }

    fn labels(specs: &[ActionSpec]) -> Vec<&'static str> {
        specs.iter().map(|spec| spec.label).collect()
    }

    fn worker(id: u128, seen: DateTime<Utc>) -> Worker {
        Worker {
            id: uuid::Uuid::from_u128(id),
            name: format!("w{id}"),
            last_seen: seen,
            version: None,
            remote_addr: None,
            telemetry: None,
            max_concurrency: 1,
            max_cpu_jobs: None,
            max_gpu_jobs: None,
            schedule: None,
            transfer_enabled: false,
        }
    }

    fn update(enabled: bool, available: bool, url: Option<&str>) -> UpdateCheckStatus {
        UpdateCheckStatus {
            enabled,
            current_version: "0.57.0".into(),
            latest_version: Some("0.58.0".into()),
            update_available: available,
            release_url: url.map(str::to_owned),
            ..UpdateCheckStatus::default()
        }
    }

    #[test]
    fn a_single_action_is_the_primary_one() {
        let layout = split_actions(vec![spec("Nouveau flux")]);
        assert_eq!(
            layout.primary.map(|action| action.label),
            Some("Nouveau flux")
        );
        assert!(layout.back.is_none() && layout.more.is_empty());
    }

    #[test]
    fn extra_actions_go_to_the_menu_and_the_back_action_stays_apart() {
        let layout = split_actions(vec![
            spec("Retour aux flux"),
            spec("Importer un fichier"),
            spec("Modèle de blocs"),
            spec("Ajouter un dépôt"),
        ]);
        assert_eq!(
            layout.back.map(|action| action.label),
            Some("Retour aux flux")
        );
        assert_eq!(
            layout.primary.map(|action| action.label),
            Some("Importer un fichier")
        );
        assert_eq!(
            labels(&layout.more),
            vec!["Modèle de blocs", "Ajouter un dépôt"]
        );
    }

    #[test]
    fn no_action_leaves_every_slot_empty() {
        let layout = split_actions(Vec::new());
        assert!(layout.back.is_none() && layout.primary.is_none() && layout.more.is_empty());
    }

    #[test]
    fn status_is_nominal_without_workers_data_or_update() {
        let summary = summarize_status(None, None, Utc::now());
        assert_eq!(summary.tone, StatusTone::Ok);
        assert_eq!((summary.workers, summary.update), (None, None));
    }

    #[test]
    fn no_online_worker_is_critical_even_with_an_update() {
        let now = Utc::now();
        let stale = worker(1, now - chrono::Duration::minutes(10));
        let info = update(true, true, None);
        let summary = summarize_status(Some(&[stale]), Some(&info), now);
        assert_eq!(summary.tone, StatusTone::Danger);
        assert_eq!(summary.workers.as_deref(), Some("Aucun worker en ligne"));
    }

    #[test]
    fn an_available_update_is_a_warning_and_keeps_its_link() {
        let now = Utc::now();
        let online = [worker(1, now), worker(2, now)];
        let info = update(true, true, Some("https://example.test/release"));
        let summary = summarize_status(Some(&online), Some(&info), now);
        assert_eq!(summary.tone, StatusTone::Warning);
        assert_eq!(summary.workers.as_deref(), Some("2 workers en ligne"));
        assert_eq!(
            summary.update,
            Some(UpdateNotice {
                label: "Mise à jour v0.58.0".into(),
                title: "Nouvelle version v0.58.0 disponible (installée : v0.57.0)".into(),
                url: Some("https://example.test/release".into()),
            })
        );
    }

    #[test]
    fn a_disabled_or_absent_update_is_not_reported() {
        let now = Utc::now();
        let online = [worker(1, now)];
        for info in [update(false, true, None), update(true, false, None)] {
            let summary = summarize_status(Some(&online), Some(&info), now);
            assert_eq!(summary.tone, StatusTone::Ok);
            assert_eq!(summary.update, None);
            assert_eq!(summary.workers.as_deref(), Some("1 worker en ligne"));
        }
    }
}
