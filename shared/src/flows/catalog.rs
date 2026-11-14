//! Blocs que le moteur sait exécuter, et les réglages qu'il lit pour chacun.
//!
//! L'éditeur génère ses formulaires depuis ces fiches ; un champ absent ici n'est pas lu par le
//! moteur. Les blocs des dépôts de plugins (mêmes identifiants ADR 0006) enrichissent la palette
//! via l'API ; une fiche native garantit l'exécution même sans dépôt synchronisé.

use super::FlowBlockKind;
use serde_json::{Value, json};

/// Nature d'un champ de réglage, et donc de son contrôle de saisie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Bool,
    /// Liste de choix : `(valeur, libellé)`.
    Select(&'static [(&'static str, &'static str)]),
    /// Dictionnaire nom → valeur (en-têtes HTTP) ; une valeur laissée vide garde l'ancienne.
    Headers,
}

/// Réglage d'un bloc, lu par le moteur sous `config.<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub help: &'static str,
}

/// Fiche d'un bloc exécutable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockSpec {
    pub kind: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    /// Le bloc reçoit-il une liaison entrante ? (faux pour l'entrée du flux)
    pub has_input: bool,
    /// Sorties nommées ; vide = une sortie simple, sauf si `terminal`.
    pub ports: &'static [(&'static str, &'static str)],
    /// Le flux s'arrête après ce bloc.
    pub terminal: bool,
    pub fields: &'static [FieldSpec],
}

impl BlockSpec {
    /// Réglages initiaux d'un bloc ajouté : la valeur par défaut que le moteur appliquerait.
    pub fn default_config(&self) -> Value {
        let mut config = serde_json::Map::new();
        for field in self.fields {
            let value = match field.kind {
                FieldKind::Bool => {
                    // Aligné sur les défauts des manifestes plugins (remux MKV).
                    json!(field.name == "disable_track_stats" || field.name == "faststart")
                }
                FieldKind::Headers => json!({}),
                FieldKind::Select(options) => {
                    json!(options.first().map_or("", |(value, _)| *value))
                }
                FieldKind::Text => match field.name {
                    "audio_languages" | "subtitle_languages" => json!("all"),
                    _ => json!(""),
                },
            };
            config.insert(field.name.to_owned(), value);
        }
        Value::Object(config)
    }

    /// Champs obligatoires restés vides dans `config`.
    pub fn missing_fields(&self, config: &Value) -> Vec<&'static str> {
        self.fields
            .iter()
            .filter(|field| field.required)
            .filter(|field| {
                config
                    .get(field.name)
                    .and_then(Value::as_str)
                    .is_none_or(|text| text.trim().is_empty())
            })
            .map(|field| field.label)
            .collect()
    }
}

const FAIL_ON_ERROR: FieldSpec = FieldSpec {
    name: "fail_on_error",
    label: "Échec bloquant",
    kind: FieldKind::Bool,
    required: false,
    help: "Coché, un échec fait échouer la tâche ; sinon il est seulement journalisé.",
};

const CONDITION_PORTS: &[(&str, &str)] = &[("true", "✓ Vrai"), ("false", "✗ Faux")];

const ARR_FIELDS: &[FieldSpec] = &[FAIL_ON_ERROR];

const QUALITY_OPTIONS: &[(&str, &str)] = &[
    ("6", "6 — défaut"),
    ("1", "1 — minimal"),
    ("2", "2"),
    ("3", "3"),
    ("4", "4"),
    ("5", "5"),
    ("7", "7"),
    ("8", "8"),
    ("9", "9 — maximal"),
];

const SPEED_OPTIONS: &[(&str, &str)] = &[
    ("3", "3 — défaut"),
    ("1", "1 — lent"),
    ("2", "2"),
    ("4", "4"),
    ("5", "5 — rapide"),
];

const VIDEO_ENCODE_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        name: "codec",
        label: "Codec",
        kind: FieldKind::Select(&[
            ("h265", "HEVC (Automatic)"),
            ("h264", "H.264"),
            ("h265_10bit", "HEVC (10-Bit)"),
            ("av1", "AV1"),
            ("av1_10bit", "AV1 (10-Bit)"),
            ("vp9", "VP9"),
        ]),
        required: false,
        help: "Codec de sortie ; HEVC par défaut.",
    },
    FieldSpec {
        name: "encoder",
        label: "Encodeur",
        kind: FieldKind::Select(&[
            ("auto", "Automatique"),
            ("cpu", "Processeur (CPU)"),
            ("nvidia", "NVIDIA (NVENC)"),
            ("intel_qsv", "Intel QSV"),
            ("vaapi", "VAAPI (Linux)"),
            ("amd_amf", "AMD AMF"),
        ]),
        required: false,
        help: "Un encodeur matériel indisponible fait échouer l'étape.",
    },
    FieldSpec {
        name: "quality",
        label: "Qualité",
        kind: FieldKind::Select(QUALITY_OPTIONS),
        required: false,
        help: "Plus élevé = meilleure qualité et fichier plus volumineux.",
    },
    FieldSpec {
        name: "speed",
        label: "Vitesse",
        kind: FieldKind::Select(SPEED_OPTIONS),
        required: false,
        help: "Plus rapide = compression moins efficace.",
    },
    FieldSpec {
        name: "custom_parameters",
        label: "Paramètres personnalisés (FFmpeg)",
        kind: FieldKind::Text,
        required: false,
        help: "Arguments FFmpeg additionnels, ex. -tune film -bf 3.",
    },
];

const AUDIO_ENCODE_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        name: "codec",
        label: "Format / Codec Audio",
        kind: FieldKind::Select(&[
            ("aac", "AAC (Universel)"),
            ("eac3", "E-AC3 / Dolby Digital Plus"),
            ("ac3", "AC3 / Dolby Digital"),
            ("flac", "FLAC (Sans perte)"),
            ("mp3", "MP3"),
            ("opus", "Opus"),
        ]),
        required: false,
        help: "",
    },
    FieldSpec {
        name: "bitrate",
        label: "Débit binaire (kbps)",
        kind: FieldKind::Select(&[
            ("192", "192 kbps (Recommandé)"),
            ("128", "128 kbps"),
            ("256", "256 kbps"),
            ("320", "320 kbps (Haute qualité)"),
            ("640", "640 kbps (Dolby 5.1)"),
        ]),
        required: false,
        help: "",
    },
    FieldSpec {
        name: "channels",
        label: "Canaux",
        kind: FieldKind::Select(&[
            ("preserve", "Conserver la disposition d'origine"),
            ("stereo", "Stéréo (2.0)"),
            ("5.1", "Surround (5.1)"),
        ]),
        required: false,
        help: "",
    },
    FieldSpec {
        name: "normalize",
        label: "Normalisation sonore (EBU R128)",
        kind: FieldKind::Bool,
        required: false,
        help: "Égalise le niveau sonore (loudnorm).",
    },
    FieldSpec {
        name: "custom_parameters",
        label: "Paramètres audio personnalisés (FFmpeg)",
        kind: FieldKind::Text,
        required: false,
        help: "Arguments FFmpeg audio additionnels.",
    },
];

/// Première valeur de chaque select = défaut sain (qualité 6, vitesse 3).
const _: () = {
    assert!(QUALITY_OPTIONS[0].0.as_bytes()[0] == b'6');
    assert!(SPEED_OPTIONS[0].0.as_bytes()[0] == b'3');
};

const REMUX_MKV_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        name: "disable_track_stats",
        label: "Désactiver les tags de statistiques de pistes",
        kind: FieldKind::Bool,
        required: false,
        help: "",
    },
    FieldSpec {
        name: "audio_languages",
        label: "Langues audio à conserver",
        kind: FieldKind::Text,
        required: false,
        help: "Codes ISO séparés par des virgules, ou all.",
    },
    FieldSpec {
        name: "subtitle_languages",
        label: "Langues de sous-titres à conserver",
        kind: FieldKind::Text,
        required: false,
        help: "Codes ISO séparés par des virgules, all, ou none.",
    },
    FieldSpec {
        name: "custom_parameters",
        label: "Paramètres personnalisés (mkvmerge)",
        kind: FieldKind::Text,
        required: false,
        help: "Arguments mkvmerge additionnels.",
    },
];

const CATALOG: &[BlockSpec] = &[
    BlockSpec {
        kind: "file-input",
        label: "Fichier source",
        category: "Entrée",
        description: "Point de départ : le fichier de la tâche, inspecté par la sonde média.",
        has_input: false,
        ports: &[],
        terminal: false,
        fields: &[],
    },
    BlockSpec {
        kind: "condition",
        label: "Condition",
        category: "Logique",
        description: "Aiguille vers « Vrai » ou « Faux » selon les métadonnées du média.",
        has_input: true,
        ports: CONDITION_PORTS,
        terminal: false,
        fields: &[
            FieldSpec {
                name: "property",
                label: "Propriété",
                kind: FieldKind::Select(&[
                    ("video.codec", "Codec vidéo"),
                    ("audio.language", "Langue audio"),
                    ("any", "N'importe quelle métadonnée"),
                ]),
                required: true,
                help: "Une piste correspondante suffit.",
            },
            FieldSpec {
                name: "operator",
                label: "Opérateur",
                kind: FieldKind::Select(&[
                    ("eq", "Contient la valeur"),
                    ("neq", "Ne contient pas la valeur"),
                ]),
                required: true,
                help: "La comparaison ignore la casse.",
            },
            FieldSpec {
                name: "target",
                label: "Valeur",
                kind: FieldKind::Text,
                required: true,
                help: "Exemple : hevc, fre.",
            },
        ],
    },
    BlockSpec {
        kind: "block.media.inspect",
        label: "Inspection média",
        category: "Média",
        description: "Réinspecte le fichier de travail (la sonde initiale reste implicite).",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: &[],
    },
    BlockSpec {
        kind: "mkvpropedit",
        label: "Édition des en-têtes",
        category: "MKVToolNix",
        description: "Modifie les en-têtes Matroska sur place, sans copie du fichier.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: &[
            FieldSpec {
                name: "default_audio_lang",
                label: "Langue audio par défaut",
                kind: FieldKind::Text,
                required: false,
                help: "Code ISO 639-2, par exemple fre.",
            },
            FieldSpec {
                name: "force_subtitles",
                label: "Marquer les sous-titres forcés",
                kind: FieldKind::Bool,
                required: false,
                help: "",
            },
        ],
    },
    BlockSpec {
        kind: "block.video.remux-mkv",
        label: "Remuxage Matroska",
        category: "MKVToolNix",
        description: "Produit un fichier de travail remuxé par mkvmerge (outil distribué).",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: REMUX_MKV_FIELDS,
    },
    BlockSpec {
        kind: "block.video.remux-mp4",
        label: "Remuxage MP4",
        category: "Média",
        description: "Remuxage MP4 via l'outil distribué (MP4Box) ; non exécuté tant que l'outil n'est pas prêt.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: &[
            FieldSpec {
                name: "faststart",
                label: "Optimisation streaming (moov au début)",
                kind: FieldKind::Bool,
                required: false,
                help: "",
            },
            FieldSpec {
                name: "custom_parameters",
                label: "Paramètres personnalisés (MP4Box)",
                kind: FieldKind::Text,
                required: false,
                help: "",
            },
        ],
    },
    BlockSpec {
        kind: "block.video.encode",
        label: "Encodage vidéo",
        category: "Média",
        description: "Encode la vidéo via FFmpeg distribué (plugins), avec codec, encodeur, qualité et vitesse.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: VIDEO_ENCODE_FIELDS,
    },
    BlockSpec {
        kind: "block.audio.encode",
        label: "Encodage audio",
        category: "Média",
        description: "Transcode l'audio via FFmpeg distribué (plugins).",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: AUDIO_ENCODE_FIELDS,
    },
    BlockSpec {
        kind: "webhook",
        label: "Webhook",
        category: "Notification",
        description: "Appelle une URL avec le contexte de la tâche.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: &[
            FieldSpec {
                name: "url",
                label: "URL",
                kind: FieldKind::Text,
                required: true,
                help: "Adresse https appelée à ce point du flux.",
            },
            FieldSpec {
                name: "method",
                label: "Méthode",
                kind: FieldKind::Select(&[
                    ("POST", "POST"),
                    ("PUT", "PUT"),
                    ("PATCH", "PATCH"),
                    ("GET", "GET"),
                ]),
                required: false,
                help: "",
            },
            FieldSpec {
                name: "custom_headers",
                label: "En-têtes",
                kind: FieldKind::Headers,
                required: false,
                help: "Chiffrés au repos ; une valeur laissée vide garde la valeur enregistrée.",
            },
            FAIL_ON_ERROR,
        ],
    },
    BlockSpec {
        kind: "sonarr-rescan",
        label: "Sonarr : réanalyser",
        category: "Intégrations",
        description: "Demande à Sonarr de réanalyser la série du fichier.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: ARR_FIELDS,
    },
    BlockSpec {
        kind: "sonarr-rename",
        label: "Sonarr : renommer",
        category: "Intégrations",
        description: "Demande à Sonarr de renommer les fichiers de la série.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: ARR_FIELDS,
    },
    BlockSpec {
        kind: "radarr-rescan",
        label: "Radarr : réanalyser",
        category: "Intégrations",
        description: "Demande à Radarr de réanalyser le film du fichier.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: ARR_FIELDS,
    },
    BlockSpec {
        kind: "radarr-rename",
        label: "Radarr : renommer",
        category: "Intégrations",
        description: "Demande à Radarr de renommer les fichiers du film.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: ARR_FIELDS,
    },
    BlockSpec {
        kind: "file-output",
        label: "Fin du flux",
        category: "Sortie",
        description: "Termine le flux : le fichier de travail devient le résultat de la tâche.",
        has_input: true,
        ports: &[],
        terminal: true,
        fields: &[],
    },
];

/// Tous les blocs exécutables, dans l'ordre de la palette.
pub fn block_catalog() -> &'static [BlockSpec] {
    CATALOG
}

/// Fiche d'un type de bloc ; `None` pour un bloc sans exécution native.
pub fn block_spec(kind: &FlowBlockKind) -> Option<&'static BlockSpec> {
    CATALOG.iter().find(|spec| spec.kind == kind.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_kind_has_one_spec_and_unknown_kinds_have_none() {
        for spec in block_catalog() {
            let kind = FlowBlockKind::parse(spec.kind);
            assert!(!matches!(kind, FlowBlockKind::Other(_)), "{}", spec.kind);
            assert_eq!(block_spec(&kind).map(|found| found.kind), Some(spec.kind));
        }
        assert_eq!(block_catalog().len(), 14);
        assert!(block_spec(&FlowBlockKind::parse("macro.unknown")).is_none());
        assert_eq!(
            block_spec(&FlowBlockKind::parse("video-encode")).map(|spec| spec.kind),
            Some("block.video.encode")
        );
    }

    #[test]
    fn default_config_fills_every_field_and_reports_required_ones() {
        let webhook = block_spec(&FlowBlockKind::Webhook);
        let config = webhook.map(BlockSpec::default_config).unwrap_or_default();
        assert_eq!(config["method"], "POST");
        assert_eq!(config["custom_headers"], json!({}));
        assert_eq!(config["fail_on_error"], false);
        assert_eq!(
            webhook.map(|spec| spec.missing_fields(&config)),
            Some(vec!["URL"])
        );
        let filled = json!({"url": "https://hook.example"});
        assert_eq!(
            webhook.map(|spec| spec.missing_fields(&filled)),
            Some(vec![])
        );
    }

    #[test]
    fn only_the_condition_branches_and_only_the_output_ends() {
        for spec in block_catalog() {
            assert_eq!(!spec.ports.is_empty(), spec.kind == "condition");
            assert_eq!(spec.terminal, spec.kind == "file-output");
            assert_eq!(!spec.has_input, spec.kind == "file-input");
        }
    }
}
