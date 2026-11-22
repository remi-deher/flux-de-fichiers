//! Composants factorisés et helpers de présentation de la section Plugins (Zéro Boilerplate).

use super::plugin_icon::PluginIcon;
use crate::ui::focus::on_activate;
use crate::ui::{Icon, IconName, Tag, TagTone};
use chrono::NaiveDate;
use forgearr_shared::format::format_day;
use forgearr_shared::plugins::{CatalogReadiness, ToolStatus};
use leptos::prelude::*;

const CARD_ICON_SIZE: u32 = 20;

/// Ton visuel d'un bandeau d'état ou d'une tuile du Store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoreTone {
    #[default]
    Neutral,
    Primary,
    Success,
    Warning,
    Danger,
}

impl StoreTone {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Primary => "primary",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}

/// Libellé et ton de l'état opérationnel d'une entrée du catalogue.
pub fn readiness_badge(
    readiness: CatalogReadiness,
    missing_tools: usize,
) -> (String, TagTone, StoreTone) {
    match readiness {
        CatalogReadiness::Ready => ("Prêt".to_owned(), TagTone::Success, StoreTone::Success),
        CatalogReadiness::NeedsTools => {
            let label = if missing_tools > 1 {
                format!("{missing_tools} outils requis")
            } else {
                "Outil requis".to_owned()
            };
            (label, TagTone::Warning, StoreTone::Warning)
        }
        CatalogReadiness::NeedsConfig => (
            "À configurer".to_owned(),
            TagTone::Primary,
            StoreTone::Primary,
        ),
        CatalogReadiness::Available => ("Modèle".to_owned(), TagTone::Neutral, StoreTone::Neutral),
    }
}

/// Libellé et ton d'un `ToolStatus` pour les `Tag` unifiés.
pub fn tool_status_tag(status: ToolStatus) -> (&'static str, TagTone, StoreTone) {
    match status {
        ToolStatus::Ready => ("Installé", TagTone::Success, StoreTone::Success),
        ToolStatus::Installing => ("Installation…", TagTone::Warning, StoreTone::Warning),
        ToolStatus::NotInstalled => ("Non installé", TagTone::Neutral, StoreTone::Neutral),
    }
}

/// Bandeau d'état réutilisable en tête des tiroirs ou en retour d'action.
#[component]
pub fn StatusBanner(
    #[prop(into)] title: String,
    #[prop(optional_no_strip)] text: Option<String>,
    #[prop(optional)] tone: StoreTone,
    #[prop(optional_no_strip)] action: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="status-banner" data-tone=tone.as_attr() role="status">
            <div class="status-banner__content">
                <p class="status-banner__title">{title}</p>
                {text.map(|msg| view! { <p class="status-banner__text">{msg}</p> })}
            </div>
            {action}
        </div>
    }
}

/// Ligne « terme / valeur » réutilisable dans les grilles de métadonnées des tiroirs.
#[component]
pub fn MetaRow(
    term: &'static str,
    #[prop(optional)] mono: bool,
    children: Children,
) -> impl IntoView {
    let val_class = if mono {
        "detail-val text-mono"
    } else {
        "detail-val"
    };
    view! {
        <div class="detail-term">{term}</div>
        <div class=val_class>{children()}</div>
    }
}

/// Ligne de date optionnelle dans une grille de métadonnées.
pub fn meta_date_row(term: &'static str, date: Option<NaiveDate>) -> impl IntoView {
    date.map(|day| {
        view! {
            <MetaRow term=term>{format_day(day)}</MetaRow>
        }
    })
}

/// Lignes communes d'identité et compatibilité partagées entre les fiches Plugin, Pack, Outil et Élément.
#[component]
pub fn CommonInfoRows(
    #[prop(into)] id: String,
    #[prop(into)] version: String,
    updated_at: Option<NaiveDate>,
    author: Option<String>,
    license: Option<String>,
    app_min: Option<String>,
    platforms: Vec<String>,
    tags: Vec<String>,
) -> impl IntoView {
    view! {
        <MetaRow term="Identifiant" mono=true>{id}</MetaRow>
        <MetaRow term="Version" mono=true>{version}</MetaRow>
        {meta_date_row("Mise à jour", updated_at)}
        {author.map(|value| view! { <MetaRow term="Auteur">{value}</MetaRow> })}
        {license.map(|value| view! { <MetaRow term="Licence" mono=true>{value}</MetaRow> })}
        {app_min.map(|value| view! { <MetaRow term="App minimale" mono=true>{value}</MetaRow> })}
        {(!platforms.is_empty()).then(|| view! {
            <MetaRow term="Plateformes">
                <div class="cluster">
                    {platforms.into_iter().map(|plat| view! {
                        <Tag label=plat tone=TagTone::Mono />
                    }).collect_view()}
                </div>
            </MetaRow>
        })}
        {(!tags.is_empty()).then(|| view! {
            <MetaRow term="Étiquettes">
                <div class="cluster">
                    {tags.into_iter().map(|tag| view! {
                        <Tag label=tag tone=TagTone::Neutral />
                    }).collect_view()}
                </div>
            </MetaRow>
        })}
    }
}

/// Carte style App Store respectant la règle des 3 signaux max (identité, statut, contexte).
#[component]
pub fn StoreCard(
    /// Identité de la carte, pour lui rendre le focus après une relecture.
    #[prop(into)]
    focus_key: String,
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    icon: IconName,
    #[prop(optional_no_strip)] plugin_icon: Option<String>,
    #[prop(optional)] tone: StoreTone,
    #[prop(optional_no_strip)] status_badge: Option<AnyView>,
    #[prop(optional_no_strip)] context_tags: Option<AnyView>,
    #[prop(into)] footer_left: String,
    #[prop(optional_no_strip)] footer_actions: Option<AnyView>,
    #[prop(optional_no_strip)] active: Option<Signal<bool>>,
    on_open: Callback<()>,
) -> impl IntoView {
    let tooltip = title.clone();
    view! {
        <article
            class="store-card"
            class:is-open=move || active.is_some_and(|sig| sig.get())
            data-tone=tone.as_attr()
            tabindex="0"
            data-focus-key=focus_key
            on:click=move |_| on_open.run(())
            on:keydown=on_activate(move || on_open.run(()))
        >
            <div class="store-card__head">
                <span class="store-card__icon" aria-hidden="true">
                    {match plugin_icon {
                        Some(plugin_id) => view! { <PluginIcon plugin_id=plugin_id /> }.into_any(),
                        None => view! { <Icon name=icon size=CARD_ICON_SIZE /> }.into_any(),
                    }}
                </span>
                <div class="store-card__identity">
                    <div class="store-card__title-row">
                        <h3 class="store-card__title" title=tooltip>{title}</h3>
                        {status_badge}
                    </div>
                    <p class="store-card__desc">{description}</p>
                </div>
            </div>
            {context_tags.map(|tags| view! {
                <div class="store-card__meta">{tags}</div>
            })}
            <div class="store-card__foot">
                <span class="store-card__version">{footer_left}</span>
                {footer_actions.map(|actions| view! {
                    <div class="store-card__actions" on:click=|ev| ev.stop_propagation()>
                        {actions}
                    </div>
                })}
            </div>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_badge_covers_all_states() {
        assert_eq!(
            readiness_badge(CatalogReadiness::Ready, 0),
            ("Prêt".into(), TagTone::Success, StoreTone::Success)
        );
        assert_eq!(
            readiness_badge(CatalogReadiness::NeedsTools, 1),
            ("Outil requis".into(), TagTone::Warning, StoreTone::Warning)
        );
        assert_eq!(
            readiness_badge(CatalogReadiness::NeedsTools, 3),
            (
                "3 outils requis".into(),
                TagTone::Warning,
                StoreTone::Warning
            )
        );
        assert_eq!(
            readiness_badge(CatalogReadiness::NeedsConfig, 0),
            ("À configurer".into(), TagTone::Primary, StoreTone::Primary)
        );
        assert_eq!(
            readiness_badge(CatalogReadiness::Available, 0),
            ("Modèle".into(), TagTone::Neutral, StoreTone::Neutral)
        );
    }

    #[test]
    fn tool_status_tag_covers_all_variants() {
        assert_eq!(tool_status_tag(ToolStatus::Ready).0, "Installé");
        assert_eq!(tool_status_tag(ToolStatus::Installing).0, "Installation…");
        assert_eq!(tool_status_tag(ToolStatus::NotInstalled).0, "Non installé");
    }
}
