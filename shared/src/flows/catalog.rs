//! Blocs que le moteur sait exécuter, et les réglages qu'il lit pour chacun.
//!
//! L'éditeur génère ses formulaires depuis ces fiches ; un champ absent ici n'est pas lu par le
//! moteur. Les blocs d'un dépôt de plugins sans exécution native restent des `Other`.

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
                FieldKind::Bool => json!(false),
                FieldKind::Headers => json!({}),
                FieldKind::Select(options) => {
                    json!(options.first().map_or("", |(value, _)| *value))
                }
                FieldKind::Text => json!(""),
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
        kind: "remux-mkv",
        label: "Remuxage Matroska",
        category: "MKVToolNix",
        description: "Produit un fichier de travail remuxé par mkvmerge.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: &[],
    },
    BlockSpec {
        kind: "video-encode",
        label: "Transformation vidéo",
        category: "Média",
        description: "Produit un fichier de travail ; les réglages d'encodage arriveront avec les plugins.",
        has_input: true,
        ports: &[],
        terminal: false,
        fields: &[],
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
        assert_eq!(block_catalog().len(), 11);
        assert!(block_spec(&FlowBlockKind::parse("remux-mp4")).is_none());
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
