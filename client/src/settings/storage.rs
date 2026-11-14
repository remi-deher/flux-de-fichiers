//! Stockage du central : dossiers, taille maximale d'un fichier et espace disque.

use super::system::{DetailRow, StatusSection};
use flux_shared::format::format_bytes;
use flux_shared::system::{SystemStatus, VolumeSpace};
use leptos::prelude::*;

fn volume_label(volume: Option<VolumeSpace>) -> String {
    volume.map_or_else(
        || "Illisible".to_owned(),
        |space| {
            format!(
                "{} libres sur {}",
                format_bytes(space.available_bytes),
                format_bytes(space.total_bytes)
            )
        },
    )
}

fn storage_rows(status: &SystemStatus) -> Vec<DetailRow> {
    let storage = &status.storage;
    vec![
        DetailRow::mono("Configuration", storage.config_dir.clone()),
        DetailRow::mono("Données", storage.data_dir.clone()),
        DetailRow::mono("Dossier d'entrée", storage.input_dir.clone()),
        DetailRow::mono("Cache des outils", storage.tools_cache_dir.clone()),
        DetailRow::mono(
            "Plugins",
            storage
                .plugins_dir
                .clone()
                .unwrap_or_else(|| "Non défini".to_owned()),
        ),
        DetailRow::text(
            "Taille maximale d'un fichier",
            format_bytes(storage.max_file_bytes),
        ),
        DetailRow::text("Volume des données", volume_label(storage.volume)),
    ]
}

#[component]
pub fn StorageSection() -> impl IntoView {
    view! {
        <StatusSection title="Stockage" subtitle="Dossiers du central et espace disque." rows=storage_rows />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_reads_free_over_total_or_says_it_is_unreadable() {
        let space = VolumeSpace {
            total_bytes: 2048,
            available_bytes: 1024,
        };
        assert_eq!(volume_label(Some(space)), "1 Kio libres sur 2 Kio");
        assert_eq!(volume_label(None), "Illisible");
    }
}
