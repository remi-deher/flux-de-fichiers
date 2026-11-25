//! « Utilisé par » et désinstallation d'un élément installé (ADR 0029 §8).

use super::STORE_TOPICS;
use crate::api;
use crate::live::{load, use_live};
use crate::route::encode_segment;
use crate::ui::Toasts;
use forgearr_shared::plugins::store::usage::ItemUsage;
use leptos::prelude::*;

const INSTALLATIONS_PATH: &str = "/api/v1/installations";

fn installation_path(item_id: &str) -> String {
    format!("{INSTALLATIONS_PATH}/{}", encode_segment(item_id))
}

/// Texte de l'usage ; `None` si rien ne se sert de l'élément.
pub fn usage_text(usage: &ItemUsage) -> Option<String> {
    (!usage.is_empty()).then(|| format!("Utilisé par {}.", usage.summary()))
}

/// Libellé du bouton : le second clic confirme.
pub const fn uninstall_label(confirming: bool, pending: bool) -> &'static str {
    match (pending, confirming) {
        (true, _) => "Désinstallation…",
        (false, true) => "Confirmer la désinstallation",
        (false, false) => "Désinstaller",
    }
}

#[component]
pub fn InstalledActions(item_id: String, can_manage: bool) -> impl IntoView {
    let live = use_live();
    let usage = load::<ItemUsage>(
        STORE_TOPICS,
        format!("{}/usage", installation_path(&item_id)),
    );
    let confirming = RwSignal::new(false);
    let path = installation_path(&item_id);
    let uninstall = Action::new_local(move |(): &()| {
        let path = path.clone();
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match uninstall.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Élément désinstallé ; ses réglages sont conservés.");
            live.touch(STORE_TOPICS);
        }
        Some(Err(error)) => {
            confirming.set(false);
            Toasts::notify_error(error.message());
        }
        None => {}
    });
    let button = move |in_use: bool| {
        can_manage.then(|| {
            view! {
                <div class="install-plan__actions">
                    <button
                        type="button"
                        class="btn btn-danger btn-sm"
                        disabled=move || in_use || uninstall.pending().get()
                        on:click=move |_| {
                            if confirming.get() {
                                uninstall.dispatch(());
                            } else {
                                confirming.set(true);
                            }
                        }
                    >
                        {move || uninstall_label(confirming.get(), uninstall.pending().get())}
                    </button>
                </div>
            }
        })
    };
    // Sans usage lisible, ce n'est pas une installation du Store : fourni localement ou en développement.
    move || {
        usage.get().and_then(Result::ok).map(|usage| {
            let text = usage_text(&usage);
            let in_use = text.is_some();
            view! {
                <section class="install-plan__section">
                    <p class="text-dim">{text.unwrap_or_else(|| "Rien ne s'en sert.".to_owned())}</p>
                    {button(in_use)}
                </section>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_named_and_the_second_click_confirms() {
        assert_eq!(usage_text(&ItemUsage::default()), None);
        let used = ItemUsage {
            flows: vec!["Films".to_owned()],
            ..ItemUsage::default()
        };
        assert_eq!(
            usage_text(&used).as_deref(),
            Some("Utilisé par flux : Films.")
        );
        assert_eq!(uninstall_label(false, false), "Désinstaller");
        assert_eq!(uninstall_label(true, false), "Confirmer la désinstallation");
        assert_eq!(uninstall_label(true, true), "Désinstallation…");
        assert_eq!(
            installation_path("a b"),
            format!("{INSTALLATIONS_PATH}/a%20b")
        );
    }
}
