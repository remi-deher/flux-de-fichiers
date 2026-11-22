//! Écouteurs de la fenêtre liés à la durée de vie du composant qui les pose.

use leptos::ev::EventDescriptor;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Écoute `event` sur la fenêtre jusqu'au démontage du composant courant.
///
/// `window_event_listener` de Leptos laisse l'écouteur en place : après un changement de page, il
/// lirait des signaux déjà libérés et ferait paniquer l'interface.
pub fn listen_window<E>(event: E, callback: impl Fn(E::EventType) + 'static)
where
    E: EventDescriptor + 'static,
    E::EventType: JsCast,
{
    let handle = window_event_listener(event, callback);
    on_cleanup(move || handle.remove());
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    const HELPER: &str = "window_listener.rs";

    fn sources(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                sources(&path, found)?;
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
        Ok(())
    }

    #[test]
    fn window_listeners_always_go_through_the_helper() -> std::io::Result<()> {
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        )?;
        assert!(!files.is_empty());
        for path in files {
            if path.file_name().is_some_and(|name| name == HELPER) {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            assert!(
                !content.contains("window_event_listener"),
                "{} doit utiliser listen_window",
                path.display()
            );
        }
        Ok(())
    }
}
