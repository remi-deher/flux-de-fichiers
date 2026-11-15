//! Composants génériques de tuile (`ResourceCard`) et d'étiquette (`Tag`), partagés par les grilles.

use super::icon::{Icon, IconName};
use super::metric_card::MetricTone;
use leptos::prelude::*;

/// Ton visuel d'une étiquette unifiée (`Tag`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TagTone {
    #[default]
    Neutral,
    Primary,
    Success,
    Warning,
    Danger,
    Mono,
}

impl TagTone {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Primary => "primary",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
            Self::Mono => "mono",
        }
    }
}

/// Étiquette sémantique ou technique unifiée pour remplacer les styles disparates de pilules/puces.
#[component]
pub fn Tag(
    #[prop(into)] label: String,
    #[prop(optional)] tone: TagTone,
    #[prop(optional_no_strip)] icon: Option<IconName>,
) -> impl IntoView {
    view! {
        <span class="ui-tag" data-tone=tone.as_attr()>
            {icon.map(|ic| view! { <Icon name=ic size=12 /> })}
            <span>{label}</span>
        </span>
    }
}

/// Tuile de ressource unifiée : liseré gauche d'état, icône, titre, sous-titre, badge, contenu et pied.
#[component]
pub fn ResourceCard(
    #[prop(into)] title: String,
    #[prop(optional_no_strip)] subtitle: Option<String>,
    icon: IconName,
    #[prop(optional)] tone: MetricTone,
    #[prop(optional_no_strip)] badge: Option<AnyView>,
    #[prop(optional_no_strip)] active: Option<Signal<bool>>,
    #[prop(optional_no_strip)] on_click: Option<Callback<()>>,
    #[prop(into)] footer_meta: String,
    #[prop(optional_no_strip)] footer_actions: Option<AnyView>,
    children: Children,
) -> impl IntoView {
    let clickable = on_click.is_some();
    view! {
        <article
            class="resource-card"
            class:is-clickable=clickable
            class:is-open=move || active.is_some_and(|sig| sig.get())
            data-tone=tone.as_attr()
            on:click=move |_| {
                if let Some(cb) = on_click {
                    cb.run(());
                }
            }
        >
            <div class="resource-card__head">
                <span class="resource-card__icon" aria-hidden="true">
                    <Icon name=icon size=17 />
                </span>
                <div class="resource-card__titles">
                    <h3 class="resource-card__title">{title}</h3>
                    {subtitle.map(|sub| view! {
                        <span class="resource-card__subtitle">{sub}</span>
                    })}
                </div>
                {badge.map(|content| view! {
                    <div class="resource-card__badge">{content}</div>
                })}
            </div>
            <div class="resource-card__body">
                {children()}
            </div>
            <div class="resource-card__foot">
                <span class="resource-card__meta">{footer_meta}</span>
                {footer_actions.map(|actions| view! {
                    <div class="resource-card__actions" on:click=|event| event.stop_propagation()>
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
    fn tag_tone_covers_every_variant() {
        assert_eq!(TagTone::Neutral.as_attr(), "neutral");
        assert_eq!(TagTone::Primary.as_attr(), "primary");
        assert_eq!(TagTone::Success.as_attr(), "success");
        assert_eq!(TagTone::Warning.as_attr(), "warning");
        assert_eq!(TagTone::Danger.as_attr(), "danger");
        assert_eq!(TagTone::Mono.as_attr(), "mono");
    }
}
