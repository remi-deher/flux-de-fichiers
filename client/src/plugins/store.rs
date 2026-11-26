//! Store (ADR 0029) : éléments de tous les dépôts activés, homonymes compris, et ce qu'on peut en faire.

use super::metadata::{StoreCard, StoreTone};
use super::{card_description, section_badge_label, section_icon};
use crate::live::Loaded;
use crate::ui::focus::on_activate;
use crate::ui::search::{matches, query};
use crate::ui::{EmptyState, Icon, IconName, Segmented, Tag, TagTone};
use forgearr_shared::plugins::store::{InstallState, ItemRef, StoreEntry};
use forgearr_shared::plugins::vocab::CatalogItemType;
use leptos::prelude::*;
use std::collections::HashMap;

/// Filtre rapide du Store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoreFilter {
    #[default]
    All,
    NotInstalled,
    Installed,
    Updates,
}

impl StoreFilter {
    pub const ALL: &[Self] = &[
        Self::All,
        Self::NotInstalled,
        Self::Installed,
        Self::Updates,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "Tous",
            Self::NotInstalled => "Non installés",
            Self::Installed => "Installés",
            Self::Updates => "Mises à jour",
        }
    }

    pub fn accepts(self, state: &InstallState) -> bool {
        match self {
            Self::All => true,
            Self::NotInstalled => matches!(
                state,
                InstallState::Available | InstallState::PartlyInstalled { .. }
            ),
            Self::Installed => matches!(
                state,
                InstallState::Installed { .. } | InstallState::Provided
            ),
            Self::Updates => has_update(state),
        }
    }
}

fn has_update(state: &InstallState) -> bool {
    matches!(
        state,
        InstallState::Installed {
            update: Some(_),
            ..
        }
    )
}

/// Libellé et ton de l'état d'un élément.
pub fn state_badge(state: &InstallState) -> (String, TagTone) {
    match state {
        InstallState::Available => ("Disponible".to_owned(), TagTone::Neutral),
        InstallState::Installed { damaged: true, .. } => ("À réparer".to_owned(), TagTone::Danger),
        InstallState::Installed {
            update: Some(update),
            ..
        } => (format!("v{} disponible", update.version), TagTone::Warning),
        InstallState::Installed { version, .. } => {
            (format!("Installé v{version}"), TagTone::Success)
        }
        InstallState::Provided => ("Fourni localement".to_owned(), TagTone::Mono),
        InstallState::HeldByOther { origin } => (
            format!("Installé depuis {}", origin.repository_name),
            TagTone::Neutral,
        ),
        InstallState::Incomplete { .. } => ("Copie incomplète".to_owned(), TagTone::Danger),
        InstallState::PartlyInstalled { remaining } => {
            (format!("{remaining} à installer"), TagTone::Warning)
        }
    }
}

/// Action principale ; `None` quand rien n'est à faire.
pub fn primary_action(state: &InstallState) -> Option<&'static str> {
    if !state.is_actionable() {
        return None;
    }
    Some(match state {
        InstallState::Installed { damaged: true, .. } => "Réparer",
        InstallState::Installed { .. } => "Mettre à jour",
        InstallState::PartlyInstalled { .. } => "Compléter",
        _ => "Installer",
    })
}

/// Identifiants portés par plusieurs dépôts : leur carte nomme son dépôt.
fn homonyms(entries: &[StoreEntry]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for entry in entries {
        *counts.entry(entry.entry.id.clone()).or_insert(0) += 1;
    }
    counts
}

fn entry_matches(entry: &StoreEntry, text: &str) -> bool {
    let item = &entry.entry;
    let mut fields = vec![item.id.as_str(), item.name.as_str()];
    if let Some(description) = item.description.as_deref() {
        fields.push(description);
    }
    fields.push(entry.origin.repository_name.as_str());
    matches(&fields, text)
}

const FEATURED_PACK_ICON_SIZE: u32 = 22;

/// Les packs sont mis en avant en tête du Store tant qu'aucun filtre ni recherche ne restreint la vue.
fn features_packs(filter: StoreFilter, text: &str) -> bool {
    filter == StoreFilter::All && text.trim().is_empty()
}

/// Élément à installer, et le nom affiché dans l'assistant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallTarget {
    pub item: ItemRef,
    pub name: String,
}

#[component]
pub fn StoreSection(
    store: Loaded<Vec<StoreEntry>>,
    can_install: bool,
    on_install: Callback<InstallTarget>,
) -> impl IntoView {
    let filter = RwSignal::new(StoreFilter::All);
    let visible = Memo::new(move |_| {
        let entries = store.get().and_then(Result::ok).unwrap_or_default();
        let text = query();
        let repeated = homonyms(&entries);
        entries
            .into_iter()
            .filter(|entry| filter.get().accepts(&entry.install))
            .filter(|entry| entry_matches(entry, &text))
            .map(|entry| {
                let shared = repeated
                    .get(&entry.entry.id)
                    .is_some_and(|count| *count > 1);
                (entry, shared)
            })
            .collect::<Vec<_>>()
    });
    let featured = Memo::new(move |_| features_packs(filter.get(), &query()));
    view! {
        <div class="store-toolbar">
            <div class="store-toolbar__actions">
                <Segmented
                    options=StoreFilter::ALL
                    selected=filter
                    label=StoreFilter::label
                    aria_label="Filtrer le Catalogue"
                />
            </div>
        </div>
        {move || match store.get() {
            None => view! {
                <div class="empty-state-panel"><p class="text-dim">"Chargement du Catalogue…"</p></div>
            }.into_any(),
            Some(Err(error)) => view! {
                <div class="empty-state-panel"><p class="form-error">{error.message()}</p></div>
            }.into_any(),
            Some(Ok(_)) if visible.read().is_empty() => view! {
                <EmptyState
                    icon_name=IconName::Download
                    title="Aucun élément"
                    message="Ajoutez ou synchronisez un dépôt, ou changez de filtre."
                />
            }.into_any(),
            Some(Ok(_)) => {
                let (packs, others): (Vec<_>, Vec<_>) = visible
                    .get()
                    .into_iter()
                    .partition(|(entry, _)| featured.get() && entry.entry.entry_type == CatalogItemType::Pack);
                view! {
                    {(!packs.is_empty()).then(|| view! {
                        <div class="store-section">
                            <div class="store-section__head">
                                <h2 class="store-section__title">"Packs recommandés"</h2>
                                <p class="store-section__subtitle">"Ensembles prêts à l'emploi : le plan installe chacun de leurs composants"</p>
                            </div>
                            <div class="featured-packs">
                                {packs.into_iter().map(|(entry, shared)| {
                                    featured_pack_card(entry, shared, can_install, on_install)
                                }).collect_view()}
                            </div>
                        </div>
                    })}
                    <div class="store-grid">
                        {others.into_iter().map(|(entry, shared)| {
                            store_card(entry, shared, can_install, on_install)
                        }).collect_view()}
                    </div>
                }.into_any()
            }
        }}
    }
}

/// Ce que montrent les deux cartes : badge d'état, action, et cible de l'assistant.
struct CardParts {
    status: String,
    tone: TagTone,
    action: Option<&'static str>,
    target: InstallTarget,
}

fn card_parts(entry: &StoreEntry, can_install: bool) -> CardParts {
    let (status, tone) = state_badge(&entry.install);
    CardParts {
        status,
        tone,
        action: primary_action(&entry.install).filter(|_| can_install),
        target: InstallTarget {
            item: ItemRef {
                repository_id: entry.origin.repository_id.clone(),
                id: entry.entry.id.clone(),
            },
            name: entry.entry.name.clone(),
        },
    }
}

fn featured_pack_card(
    entry: StoreEntry,
    shared: bool,
    can_install: bool,
    on_install: Callback<InstallTarget>,
) -> impl IntoView {
    let CardParts {
        status,
        tone,
        action,
        target,
    } = card_parts(&entry, can_install);
    let open = {
        let target = target.clone();
        move || {
            if action.is_some() {
                on_install.run(target.clone());
            }
        }
    };
    let open_key = open.clone();
    let foot = if shared {
        format!(
            "v{} · {}",
            entry.entry.version, entry.origin.repository_name
        )
    } else {
        format!("v{}", entry.entry.version)
    };
    view! {
        <article
            class="featured-pack"
            tabindex="0"
            on:click=move |_| open()
            on:keydown=on_activate(open_key)
        >
            <div class="featured-pack__top">
                <span class="featured-pack__icon" aria-hidden="true">
                    <Icon name=IconName::Download size=FEATURED_PACK_ICON_SIZE />
                </span>
                <Tag label=status tone=tone />
            </div>
            <div>
                <h3 class="featured-pack__title">{entry.entry.name.clone()}</h3>
                <p class="featured-pack__desc">{card_description(&entry.entry)}</p>
            </div>
            <div class="featured-pack__foot">
                <span class="store-card__version">{foot}</span>
                {action.map(|label| view! {
                    <button type="button" class="btn btn-primary btn-sm" on:click=move |ev| {
                        ev.stop_propagation();
                        on_install.run(target.clone());
                    }>
                        {label}
                    </button>
                })}
            </div>
        </article>
    }
}

fn store_card(
    entry: StoreEntry,
    shared: bool,
    can_install: bool,
    on_install: Callback<InstallTarget>,
) -> impl IntoView {
    let CardParts {
        status,
        tone,
        action,
        target,
    } = card_parts(&entry, can_install);
    let StoreEntry { origin, entry, .. } = entry;
    let open = {
        let target = target.clone();
        Callback::new(move |()| {
            if action.is_some() {
                on_install.run(target.clone());
            }
        })
    };
    let actions = action.map(|label| {
        view! {
            <button type="button" class="btn btn-primary btn-sm" on:click=move |ev| {
                ev.stop_propagation();
                on_install.run(target.clone());
            }>
                {label}
            </button>
        }
        .into_any()
    });
    let context = view! {
        <Tag label=section_badge_label(entry.entry_type) tone=TagTone::Primary />
        {shared.then(|| view! { <Tag label=origin.repository_name.clone() tone=TagTone::Mono /> })}
    }
    .into_any();
    let card_tone = match tone {
        TagTone::Danger => StoreTone::Danger,
        TagTone::Warning => StoreTone::Warning,
        TagTone::Success => StoreTone::Success,
        _ => StoreTone::Neutral,
    };
    view! {
        <StoreCard
            focus_key=format!("{}:{}", origin.repository_id, entry.id)
            title=entry.name.clone()
            description=card_description(&entry)
            icon=section_icon(entry.entry_type)
            tone=card_tone
            status_badge=Some(view! { <Tag label=status tone=tone /> }.into_any())
            context_tags=Some(context)
            footer_left=format!("v{}", entry.version)
            footer_actions=actions
            on_open=open
        />
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::store::{AvailableUpdate, ItemOrigin};

    fn installed(update: Option<&str>, damaged: bool) -> InstallState {
        InstallState::Installed {
            version: "1.0.0".to_owned(),
            update: update.map(|version| AvailableUpdate {
                version: version.to_owned(),
                content_only: false,
            }),
            damaged,
        }
    }

    #[test]
    fn filters_sort_items_by_what_can_be_done() {
        let update = installed(Some("1.1.0"), false);
        assert!(StoreFilter::Updates.accepts(&update));
        assert!(StoreFilter::Installed.accepts(&update));
        assert!(!StoreFilter::NotInstalled.accepts(&update));
        assert!(StoreFilter::NotInstalled.accepts(&InstallState::Available));
        assert!(!StoreFilter::Updates.accepts(&installed(None, false)));
        assert!(StoreFilter::All.accepts(&InstallState::Provided));
    }

    #[test]
    fn packs_are_featured_only_on_the_unfiltered_store() {
        assert!(features_packs(StoreFilter::All, " "));
        assert!(!features_packs(StoreFilter::All, "hdr"));
        assert!(!features_packs(StoreFilter::Updates, ""));
    }

    #[test]
    fn the_action_follows_the_install_state() {
        assert_eq!(primary_action(&InstallState::Available), Some("Installer"));
        assert_eq!(
            primary_action(&installed(Some("2.0.0"), false)),
            Some("Mettre à jour")
        );
        assert_eq!(primary_action(&installed(None, true)), Some("Réparer"));
        assert_eq!(primary_action(&installed(None, false)), None);
        let held = InstallState::HeldByOther {
            origin: ItemOrigin {
                repository_id: "tiers".to_owned(),
                repository_url: "https://tiers.example/index.json".to_owned(),
                repository_name: "Tiers".to_owned(),
            },
        };
        assert_eq!(primary_action(&held), None);
        assert_eq!(state_badge(&held).0, "Installé depuis Tiers");
        assert_eq!(
            state_badge(&installed(Some("2.0.0"), false)).0,
            "v2.0.0 disponible"
        );
    }
}
