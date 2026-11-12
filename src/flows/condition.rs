use serde::{Deserialize, Serialize};

/// Configuration d'une condition d'aiguillage (ADR 0008).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionConfig {
    #[serde(default)]
    pub property: String,
    #[serde(default = "default_op")]
    pub operator: String,
    #[serde(default)]
    pub target: String,
}

fn default_op() -> String {
    "eq".into()
}

/// Évalue une condition d'aiguillage sur les métadonnées techniques extraites.
pub fn evaluate_condition(config: &ConditionConfig, media_info: &serde_json::Value) -> bool {
    let prop = config.property.to_lowercase();
    let op = config.operator.to_lowercase();
    let target = config.target.to_lowercase();
    let tracks = media_info["tracks"].as_array();

    let match_found = if prop == "video.codec" {
        tracks
            .map(|arr| {
                arr.iter().any(|track| {
                    track["type"].as_str() == Some("video")
                        && track["codec"]
                            .as_str()
                            .is_some_and(|codec| codec.to_lowercase().contains(&target))
                })
            })
            .unwrap_or(false)
    } else if prop == "audio.language" {
        tracks
            .map(|arr| {
                arr.iter().any(|track| {
                    track["type"].as_str() == Some("audio")
                        && track["properties"]["language"]
                            .as_str()
                            .is_some_and(|language| language.to_lowercase().contains(&target))
                })
            })
            .unwrap_or(false)
    } else {
        media_info.to_string().to_lowercase().contains(&target)
    };

    match op.as_str() {
        "eq" | "contains" => match_found,
        "neq" => !match_found,
        _ => match_found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media_info() -> serde_json::Value {
        serde_json::json!({
            "tracks": [
                {"type": "video", "codec": "HEVC"},
                {"type": "audio", "codec": "AAC", "properties": {"language": "fre"}}
            ]
        })
    }

    #[test]
    fn video_codec_eq_and_neq() {
        let info = media_info();
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "video.codec".into(),
                operator: "eq".into(),
                target: "hevc".into(),
            },
            &info
        ));
        assert!(!evaluate_condition(
            &ConditionConfig {
                property: "video.codec".into(),
                operator: "eq".into(),
                target: "h264".into(),
            },
            &info
        ));
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "video.codec".into(),
                operator: "neq".into(),
                target: "h264".into(),
            },
            &info
        ));
    }

    #[test]
    fn audio_language_contains() {
        let info = media_info();
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "audio.language".into(),
                operator: "contains".into(),
                target: "fre".into(),
            },
            &info
        ));
        assert!(!evaluate_condition(
            &ConditionConfig {
                property: "audio.language".into(),
                operator: "eq".into(),
                target: "eng".into(),
            },
            &info
        ));
    }

    #[test]
    fn unknown_operator_keeps_the_match_and_generic_property_searches_the_document() {
        let info = media_info();
        assert!(evaluate_condition(
            &ConditionConfig {
                property: "container".into(),
                operator: "gt".into(),
                target: "hevc".into(),
            },
            &info
        ));
        assert!(!evaluate_condition(
            &ConditionConfig {
                property: "container".into(),
                operator: "eq".into(),
                target: "av1".into(),
            },
            &info
        ));
    }
}
