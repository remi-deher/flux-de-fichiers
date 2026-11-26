//! État d'écran porté par l'adresse (ADR 0033, lot 2) : onglet d'un écran, tiroir ouvert.
//! Un lien copié rouvre le même onglet et le même tiroir ; Retour ferme le tiroir.

use crate::route::use_router;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Paramètre de requête qui désigne le tiroir de détail ouvert : `?voir=<type>.<id>`.
pub const DETAIL_PARAM: &str = "voir";
/// Paramètre de requête qui désigne le tiroir de création ouvert : `?nouveau=<type>`.
pub const CREATE_PARAM: &str = "nouveau";
/// Sépare le type de l'identifiant dans `?voir=flux.<id>`.
const DETAIL_SEPARATOR: char = '.';

/// Tiroirs adressables : leur nom dans l'adresse (`?voir=flux.<id>`, `?nouveau=mot-de-passe`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawerKind {
    #[serde(rename = "flux")]
    Flow,
    #[serde(rename = "modele")]
    FlowTemplate,
    #[serde(rename = "worker")]
    Worker,
    #[serde(rename = "mot-de-passe")]
    Password,
    #[serde(rename = "cle")]
    ApiKey,
    #[serde(rename = "compte")]
    Account,
    #[serde(rename = "depot")]
    Repository,
    #[serde(rename = "pack")]
    Pack,
    #[serde(rename = "ressource")]
    Resource,
}

impl DrawerKind {
    /// Nom du tiroir dans l'adresse.
    pub fn slug(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
}

/// Onglet d'un écran : son nom dans l'adresse (`/plugins/installed`) et son retour depuis ce nom.
pub trait UrlTab: Copy + PartialEq + Send + Sync + 'static {
    fn slug(self) -> &'static str;
    fn from_slug(slug: &str) -> Option<Self>;
}

/// Onglet courant d'un écran, lié à l'adresse. L'onglet par défaut n'écrit aucun segment ;
/// un nom inconnu ou un onglet non autorisé mène à l'onglet par défaut.
pub fn use_tab<T: UrlTab>(default: T, allowed: Vec<T>) -> RwSignal<T> {
    let router = use_router();
    let read = move |slug: Option<String>| {
        slug.as_deref()
            .and_then(T::from_slug)
            .filter(|tab| allowed.contains(tab))
            .unwrap_or(default)
    };
    let tab = RwSignal::new(read.clone()(router.tab_untracked()));
    Effect::new(move |_| {
        let from_url = read.clone()(router.tab());
        if tab.get_untracked() != from_url {
            tab.set(from_url);
        }
    });
    Effect::new(move |_| {
        let chosen = tab.get();
        router.set_tab((chosen != default).then(|| chosen.slug()));
    });
    tab
}

/// Paramètre de requête lié à un signal : le signal suit l'adresse, et l'adresse suit le signal.
pub fn use_param(key: &'static str) -> RwSignal<Option<String>> {
    let router = use_router();
    let value = RwSignal::new(router.query_untracked().get(key).map(str::to_owned));
    Effect::new(move |_| {
        let from_url = router.query().get(key).map(str::to_owned);
        if value.get_untracked() != from_url {
            value.set(from_url);
        }
    });
    Effect::new(move |_| router.set_param(key, value.get().as_deref()));
    value
}

/// Tiroir de création : ouvert tant que `?nouveau=<kind>` est dans l'adresse.
pub fn use_create_drawer(kind: DrawerKind) -> RwSignal<bool> {
    param_flag(use_param(CREATE_PARAM), kind.slug())
}

/// Tiroir de détail : ouvert tant que `?voir=<kind>.<id>` est dans l'adresse. Le signal de gauche
/// porte l'identifiant sélectionné, celui de droite l'ouverture ; les deux restent d'accord.
pub fn use_detail_drawer(kind: DrawerKind) -> (RwSignal<Option<String>>, RwSignal<bool>) {
    let kind = kind.slug();
    let param = use_param(DETAIL_PARAM);
    let selected = RwSignal::new(param.get_untracked().and_then(|v| detail_id(&v, &kind)));
    let open = RwSignal::new(selected.get_untracked().is_some());
    let url_kind = kind.clone();
    Effect::new(move |_| {
        let value = param.get();
        match value.as_deref().and_then(|value| detail_id(value, &url_kind)) {
            Some(id) => {
                if selected.get_untracked().as_ref() != Some(&id) {
                    selected.set(Some(id));
                }
                if !open.get_untracked() {
                    open.set(true);
                }
            }
            None if value.is_none() && open.get_untracked() => open.set(false),
            None => {}
        }
    });
    Effect::new(move |_| {
        let chosen = selected.get();
        let is_open = open.get();
        let wanted = chosen.filter(|_| is_open).map(|id| detail_value(&kind, &id));
        let current = param.get_untracked();
        // Un tiroir d'un autre type reste ouvert : on ne l'écrase que pour fermer le nôtre.
        let ours = current.as_deref().is_some_and(|v| detail_id(v, &kind).is_some());
        if wanted != current && (wanted.is_some() || ours) {
            param.set(wanted);
        }
    });
    (selected, open)
}

/// Écrit `kind.id` pour `?voir=`.
fn detail_value(kind: &str, id: &str) -> String {
    format!("{kind}{DETAIL_SEPARATOR}{id}")
}

/// Identifiant de `?voir=<kind>.<id>`, ou `None` si le paramètre désigne un autre type.
fn detail_id(value: &str, kind: &str) -> Option<String> {
    let (value_kind, id) = value.split_once(DETAIL_SEPARATOR)?;
    (value_kind == kind && !id.is_empty()).then(|| id.to_owned())
}

/// Booléen d'ouverture lié à un paramètre : vrai quand le paramètre vaut `expected`.
fn param_flag(param: RwSignal<Option<String>>, expected: String) -> RwSignal<bool> {
    let open = RwSignal::new(param.get_untracked().as_deref() == Some(expected.as_str()));
    let on_url = expected.clone();
    Effect::new(move |_| {
        let from_url = param.get().as_deref() == Some(on_url.as_str());
        if open.get_untracked() != from_url {
            open.set(from_url);
        }
    });
    Effect::new(move |_| {
        let wanted = open.get();
        let current = param.get_untracked();
        let is_ours = current.as_deref() == Some(expected.as_str());
        if wanted && !is_ours {
            param.set(Some(expected.clone()));
        } else if !wanted && is_ours {
            param.set(None);
        }
    });
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawers_are_named_in_the_address() {
        assert_eq!(DrawerKind::Flow.slug(), "flux");
        assert_eq!(DrawerKind::FlowTemplate.slug(), "modele");
        assert_eq!(DrawerKind::Password.slug(), "mot-de-passe");
        assert_eq!(DrawerKind::Resource.slug(), "ressource");
    }

    #[test]
    fn a_detail_value_carries_its_type_and_its_identifier() {
        assert_eq!(detail_value("flux", "3f2a"), "flux.3f2a");
        assert_eq!(detail_id("flux.3f2a", "flux").as_deref(), Some("3f2a"));
        // L'identifiant peut lui-même contenir le séparateur.
        assert_eq!(detail_id("flux.flow.a", "flux").as_deref(), Some("flow.a"));
        assert_eq!(detail_id("worker.9", "flux"), None);
        assert_eq!(detail_id("flux.", "flux"), None);
        assert_eq!(detail_id("flux", "flux"), None);
    }
}
