use serde::{Deserialize, Serialize};

/// Paramètres d'une édition d'en-têtes in-place (ADR 0008).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InPlaceEdit {
    pub default_audio_lang: Option<String>,
    pub force_subtitles: bool,
}

pub fn in_place_edit(config: &serde_json::Value) -> InPlaceEdit {
    InPlaceEdit {
        default_audio_lang: config["default_audio_lang"].as_str().map(str::to_owned),
        force_subtitles: config["force_subtitles"].as_bool().unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_place_edit_reads_present_fields_and_defaults_the_rest() {
        let parsed = in_place_edit(&serde_json::json!({
            "default_audio_lang": "fre",
            "force_subtitles": true
        }));
        assert_eq!(parsed.default_audio_lang.as_deref(), Some("fre"));
        assert!(parsed.force_subtitles);

        let defaults = in_place_edit(&serde_json::json!({}));
        assert_eq!(defaults.default_audio_lang, None);
        assert!(!defaults.force_subtitles);
    }
}
