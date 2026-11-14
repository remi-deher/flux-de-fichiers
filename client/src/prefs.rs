//! Préférences d'interface conservées dans le navigateur (`localStorage`).

use leptos::prelude::*;

/// Valeur enregistrée sous cette clé, si le stockage est disponible.
pub fn load(key: &str) -> Option<String> {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(key).ok().flatten())
}

/// Enregistre une valeur ; sans stockage disponible, la préférence n'est simplement pas retenue.
pub fn store(key: &str, value: &str) {
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item(key, value);
    }
}
