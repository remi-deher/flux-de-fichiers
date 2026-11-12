//! Volet latéral gauche : Palette des blocs disponibles (Rôle Construire).
//!
//! Les blocs sont déclarés dans un registre ; un type de bloc est la valeur de `type` d'un nœud
//! dans la définition d'un flux, lue par le moteur d'exécution.

use leptos::prelude::*;

/// Un bloc proposé à l'ajout.
struct PaletteBlock {
    block_type: &'static str,
    tool: &'static str,
    icon: &'static str,
    title: &'static str,
    subtitle: &'static str,
    /// Un bloc non ajoutable (le déclencheur) est affiché sans bouton « + ».
    addable: bool,
}

struct PaletteGroup {
    title: &'static str,
    blocks: &'static [PaletteBlock],
}

const fn block(
    block_type: &'static str,
    tool: &'static str,
    icon: &'static str,
    title: &'static str,
    subtitle: &'static str,
) -> PaletteBlock {
    PaletteBlock {
        block_type,
        tool,
        icon,
        title,
        subtitle,
        addable: true,
    }
}

const PALETTE_GROUPS: &[PaletteGroup] = &[
    PaletteGroup {
        title: "📁 Entrées & Sorties",
        blocks: &[
            PaletteBlock {
                addable: false,
                ..block(
                    "file-input",
                    "system",
                    "📄",
                    "Fichier entrant",
                    "Surveillance / Déclencheur",
                )
            },
            block(
                "file-output",
                "system",
                "💾",
                "Destination finale",
                "Remplacement / Déplacement",
            ),
        ],
    },
    PaletteGroup {
        title: "🔀 Logique & Conditions",
        blocks: &[block(
            "condition",
            "logic",
            "🔀",
            "Aiguillage conditionnel",
            "Vrai / Faux / Repli matériel",
        )],
    },
    PaletteGroup {
        title: "🎞️ Traitement Vidéo",
        blocks: &[
            block(
                "video-encode",
                "ffmpeg",
                "🎞️",
                "Réencoder Vidéo",
                "FFmpeg (HEVC/QSV/NVENC)",
            ),
            block(
                "remux-mkv",
                "mkvtoolnix",
                "🎬",
                "Remuxer MKV",
                "MKVToolNix (mkvmerge)",
            ),
            block(
                "mkvpropedit",
                "mkvtoolnix",
                "⚡",
                "Édition En-têtes (In-Place)",
                "mkvpropedit (Sans copie disque)",
            ),
            block(
                "remux-mp4",
                "mp4box",
                "📦",
                "Remuxer MP4",
                "MP4Box GPAC (FastStart)",
            ),
        ],
    },
    PaletteGroup {
        title: "🔊 Traitement Audio",
        blocks: &[block(
            "audio-encode",
            "ffmpeg",
            "🔊",
            "Réencoder Audio",
            "FFmpeg (AAC/Opus/AC3)",
        )],
    },
    PaletteGroup {
        title: "🔗 Intégrations (*arr & Webhooks)",
        blocks: &[
            block(
                "webhook",
                "webhook",
                "🌐",
                "Notification Webhook",
                "Envoi HTTP contextuel",
            ),
            block(
                "sonarr-rescan",
                "sonarr",
                "📡",
                "Sonarr : Actualiser la série",
                "RescanSeries",
            ),
            block(
                "sonarr-rename",
                "sonarr",
                "📡",
                "Sonarr : Renommer l'épisode",
                "RenameFiles",
            ),
            block(
                "radarr-rescan",
                "radarr",
                "🎬",
                "Radarr : Actualiser le film",
                "RescanMovie",
            ),
            block(
                "radarr-rename",
                "radarr",
                "🎬",
                "Radarr : Renommer le film",
                "RenameFiles",
            ),
        ],
    },
];

#[component]
fn PaletteItem(block: &'static PaletteBlock) -> impl IntoView {
    let class = if block.addable {
        "draggable-block-item addable"
    } else {
        "draggable-block-item"
    };
    view! {
        <div class=class data-block-type=block.block_type data-tool=block.tool>
            <span class="block-drag-icon">{block.icon}</span>
            <div class="block-drag-info">
                <strong>{block.title}</strong>
                <span>{block.subtitle}</span>
            </div>
            {block.addable.then(|| view! { <span class="block-add-plus">"+"</span> })}
        </div>
    }
}

#[component]
pub fn FlowPalette() -> impl IntoView {
    view! {
        <aside class="canvas-blocks-sidebar" aria-label="Bibliothèque de blocs disponibles">
            {PALETTE_GROUPS
                .iter()
                .map(|group| view! {
                    <div class="sidebar-block-group">
                        <span class="sidebar-group-title">{group.title}</span>
                        {group.blocks.iter().map(|block| view! { <PaletteItem block=block /> }).collect_view()}
                    </div>
                })
                .collect_view()}

            <div class="sidebar-help-tip">
                "💡 Cliquez sur un bloc avec « + » pour l'ajouter à la suite de votre chaîne."
            </div>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn render() -> String {
        view! { <FlowPalette /> }.to_html()
    }

    #[test]
    fn test_block_types_are_unique() {
        let types: Vec<_> = PALETTE_GROUPS
            .iter()
            .flat_map(|group| group.blocks.iter().map(|block| block.block_type))
            .collect();
        let unique: HashSet<_> = types.iter().collect();
        assert_eq!(unique.len(), types.len());
    }

    #[test]
    fn test_arr_blocks_use_the_types_read_by_the_engine() {
        let html = render();
        for block_type in [
            "sonarr-rescan",
            "sonarr-rename",
            "radarr-rescan",
            "radarr-rename",
        ] {
            assert!(
                html.contains(&format!("data-block-type=\"{block_type}\"")),
                "{block_type}"
            );
        }
        assert!(!html.contains("data-block-type=\"sonarr\""));
        assert!(!html.contains("data-block-type=\"radarr\""));
    }

    #[test]
    fn test_only_the_trigger_has_no_add_button() {
        let html = render();
        let addable = PALETTE_GROUPS
            .iter()
            .flat_map(|group| group.blocks.iter())
            .filter(|block| block.addable)
            .count();
        assert_eq!(html.matches("block-add-plus").count(), addable);
        assert!(html.contains("data-block-type=\"file-input\""));
    }

    #[test]
    fn test_palette_has_no_inline_style() {
        assert!(!render().contains("style="));
    }
}
