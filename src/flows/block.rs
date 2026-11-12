use super::{
    condition::{ConditionConfig, evaluate_condition},
    tool::{InPlaceEdit, in_place_edit},
    webhook::WebhookBlockConfig,
};
use serde::{Deserialize, Serialize, de::Deserializer, ser::Serializer};
use std::fmt;

/// Type de bloc d'un graphe de flux. Les valeurs inconnues restent traversables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowBlockKind {
    FileInput,
    Condition,
    Mkvpropedit,
    RemuxMkv,
    VideoEncode,
    Webhook,
    FileOutput,
    Other(String),
}

impl FlowBlockKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "file-input" => Self::FileInput,
            "condition" => Self::Condition,
            "mkvpropedit" => Self::Mkvpropedit,
            "remux-mkv" => Self::RemuxMkv,
            "video-encode" => Self::VideoEncode,
            "webhook" => Self::Webhook,
            "file-output" => Self::FileOutput,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::FileInput => "file-input",
            Self::Condition => "condition",
            Self::Mkvpropedit => "mkvpropedit",
            Self::RemuxMkv => "remux-mkv",
            Self::VideoEncode => "video-encode",
            Self::Webhook => "webhook",
            Self::FileOutput => "file-output",
            Self::Other(value) => value,
        }
    }
}

impl fmt::Display for FlowBlockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for FlowBlockKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FlowBlockKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// Effet d'un bloc, sans exécuter d'outil ni d'appel réseau.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepDecision {
    pub port: Option<String>,
    pub stop: bool,
    pub transform: bool,
    pub edit: Option<InPlaceEdit>,
    pub notify: Option<WebhookBlockConfig>,
}

impl StepDecision {
    fn pass() -> Self {
        Self {
            port: None,
            stop: false,
            transform: false,
            edit: None,
            notify: None,
        }
    }

    fn stop() -> Self {
        Self {
            stop: true,
            ..Self::pass()
        }
    }

    fn branch(matched: bool) -> Self {
        Self {
            port: Some(if matched { "true" } else { "false" }.to_owned()),
            ..Self::pass()
        }
    }

    fn transform() -> Self {
        Self {
            transform: true,
            ..Self::pass()
        }
    }

    fn edit(edit: InPlaceEdit) -> Self {
        Self {
            edit: Some(edit),
            ..Self::pass()
        }
    }

    fn notify(config: WebhookBlockConfig) -> Self {
        Self {
            notify: Some(config),
            ..Self::pass()
        }
    }
}

pub fn decide_step(
    kind: &FlowBlockKind,
    config: &serde_json::Value,
    media_info: &serde_json::Value,
) -> StepDecision {
    match kind {
        FlowBlockKind::FileInput | FlowBlockKind::Other(_) => StepDecision::pass(),
        FlowBlockKind::FileOutput => StepDecision::stop(),
        FlowBlockKind::Condition => {
            let parsed =
                serde_json::from_value::<ConditionConfig>(config.clone()).unwrap_or_else(|_| {
                    ConditionConfig {
                        property: "video.codec".into(),
                        operator: "eq".into(),
                        target: "hevc".into(),
                    }
                });
            StepDecision::branch(evaluate_condition(&parsed, media_info))
        }
        FlowBlockKind::Mkvpropedit => StepDecision::edit(in_place_edit(config)),
        FlowBlockKind::RemuxMkv | FlowBlockKind::VideoEncode => StepDecision::transform(),
        FlowBlockKind::Webhook => {
            match serde_json::from_value::<WebhookBlockConfig>(config.clone()) {
                Ok(webhook) => StepDecision::notify(webhook),
                Err(_) => StepDecision::pass(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip_keeps_unknown_blocks() -> Result<(), serde_json::Error> {
        for label in [
            "file-input",
            "condition",
            "mkvpropedit",
            "remux-mkv",
            "video-encode",
            "webhook",
            "file-output",
            "remux-mp4",
        ] {
            let kind = FlowBlockKind::parse(label);
            assert_eq!(kind.as_str(), label);
            assert_eq!(
                serde_json::from_str::<FlowBlockKind>(&serde_json::to_string(&kind)?)?,
                kind
            );
        }
        assert!(matches!(
            FlowBlockKind::parse("remux-mp4"),
            FlowBlockKind::Other(_)
        ));
        Ok(())
    }

    #[test]
    fn each_block_kind_plans_a_single_effect() {
        let hevc = serde_json::json!({"tracks": [{"type": "video", "codec": "HEVC"}]});
        let empty = serde_json::json!({});

        assert!(
            decide_step(&FlowBlockKind::FileInput, &empty, &hevc)
                .port
                .is_none()
        );
        assert!(!decide_step(&FlowBlockKind::FileInput, &empty, &hevc).stop);

        let other = decide_step(&FlowBlockKind::Other("remux-mp4".into()), &empty, &hevc);
        assert!(!other.stop && !other.transform && other.notify.is_none());

        assert!(decide_step(&FlowBlockKind::FileOutput, &empty, &hevc).stop);

        let matched = decide_step(
            &FlowBlockKind::Condition,
            &serde_json::json!({"property": "video.codec", "operator": "eq", "target": "hevc"}),
            &hevc,
        );
        assert_eq!(matched.port.as_deref(), Some("true"));

        let fallback = decide_step(&FlowBlockKind::Condition, &serde_json::json!({}), &hevc);
        assert_eq!(fallback.port.as_deref(), Some("true"));

        let edit = decide_step(
            &FlowBlockKind::Mkvpropedit,
            &serde_json::json!({"default_audio_lang": "fre", "force_subtitles": true}),
            &hevc,
        );
        let edit = edit.edit.as_ref();
        assert_eq!(
            edit.and_then(|value| value.default_audio_lang.as_deref()),
            Some("fre")
        );
        assert_eq!(edit.map(|value| value.force_subtitles), Some(true));

        assert!(decide_step(&FlowBlockKind::RemuxMkv, &empty, &hevc).transform);
        assert!(decide_step(&FlowBlockKind::VideoEncode, &empty, &hevc).transform);

        let notify = decide_step(
            &FlowBlockKind::Webhook,
            &serde_json::json!({"url": "http://127.0.0.1/hook", "method": "PUT"}),
            &hevc,
        );
        assert_eq!(
            notify.notify.as_ref().map(|hook| hook.method.as_str()),
            Some("PUT")
        );
        assert!(
            decide_step(
                &FlowBlockKind::Webhook,
                &serde_json::json!({"method": "POST"}),
                &hevc
            )
            .notify
            .is_none()
        );
    }
}
