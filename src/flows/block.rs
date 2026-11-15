use super::{
    arr::{ArrAction, ArrBlock, ArrService},
    condition::{ConditionConfig, evaluate_condition},
    encode::EncodePlan,
    mp4box::{Mp4Plan, mp4_plan},
    tool::{InPlaceEdit, in_place_edit},
    webhook::WebhookBlockConfig,
};

pub use flux_shared::flows::FlowBlockKind;

/// Effet d'un bloc, sans exécuter d'outil ni d'appel réseau.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepDecision {
    pub port: Option<String>,
    pub stop: bool,
    pub transform: bool,
    pub encode: Option<EncodePlan>,
    pub remux_mp4: Option<Mp4Plan>,
    pub reinpect: bool,
    pub edit: Option<InPlaceEdit>,
    pub notify: Option<WebhookBlockConfig>,
    pub integration: Option<ArrBlock>,
}

impl StepDecision {
    fn pass() -> Self {
        Self {
            port: None,
            stop: false,
            transform: false,
            encode: None,
            remux_mp4: None,
            reinpect: false,
            edit: None,
            notify: None,
            integration: None,
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

    fn remux_mp4(plan: Mp4Plan) -> Self {
        Self {
            remux_mp4: Some(plan),
            ..Self::pass()
        }
    }

    fn encode(plan: EncodePlan) -> Self {
        Self {
            encode: Some(plan),
            ..Self::pass()
        }
    }

    fn reinpect() -> Self {
        Self {
            reinpect: true,
            ..Self::pass()
        }
    }

    fn edit(edit: InPlaceEdit) -> Self {
        Self {
            edit: Some(edit),
            ..Self::pass()
        }
    }

    fn integrate(block: ArrBlock) -> Self {
        Self {
            integration: Some(block),
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

/// Bloc d'intégration : la politique d'échec vient de `config.fail_on_error` (faux par défaut).
fn arr_step(service: ArrService, action: ArrAction, config: &serde_json::Value) -> StepDecision {
    let fail_on_error = config
        .get("fail_on_error")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    StepDecision::integrate(ArrBlock {
        service,
        action,
        fail_on_error,
    })
}

pub fn is_entry_block(kind: &FlowBlockKind) -> bool {
    matches!(kind.as_str(), "file-input" | "block.core.file-input")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockOperation {
    Pass,
    Stop,
    MediaInspect,
    Condition,
    Mkvpropedit,
    RemuxMkv,
    RemuxMp4,
    VideoEncode,
    AudioEncode,
    SonarrRescan,
    SonarrRename,
    RadarrRescan,
    RadarrRename,
    Webhook,
}

impl BlockOperation {
    /// Identifiant canonique d'opération issu du contrat de plugin.
    pub const fn operation_id(self) -> &'static str {
        match self {
            Self::Pass => "operation.core.file-input",
            Self::Stop => "operation.core.file-output",
            Self::MediaInspect => "operation.mediainfo.probe",
            Self::Condition => "operation.core.condition",
            Self::Mkvpropedit => "operation.mkvtoolnix.mkvpropedit",
            Self::RemuxMkv => "operation.mkvtoolnix.mkvmerge.remux",
            Self::RemuxMp4 => "operation.mp4box.remux",
            Self::VideoEncode => "operation.ffmpeg.video.encode",
            Self::AudioEncode => "operation.ffmpeg.audio.encode",
            Self::SonarrRescan => "operation.sonarr.rescan",
            Self::SonarrRename => "operation.sonarr.rename",
            Self::RadarrRescan => "operation.radarr.rescan",
            Self::RadarrRename => "operation.radarr.rename",
            Self::Webhook => "operation.core.webhook",
        }
    }

    /// Outil distribué requis par cette opération, le cas échéant.
    pub const fn required_tool_id(self) -> Option<&'static str> {
        match self {
            Self::Mkvpropedit | Self::RemuxMkv => Some(crate::plugins::REMUX_TOOL_ID),
            Self::RemuxMp4 => Some(crate::plugins::MP4BOX_TOOL_ID),
            Self::VideoEncode | Self::AudioEncode => Some(crate::plugins::FFMPEG_TOOL_ID),
            Self::MediaInspect
            | Self::Pass
            | Self::Stop
            | Self::Condition
            | Self::SonarrRescan
            | Self::SonarrRename
            | Self::RadarrRescan
            | Self::RadarrRename
            | Self::Webhook => None,
        }
    }

    /// Service *arr associé à cette opération, le cas échéant.
    pub const fn arr_service(self) -> Option<ArrService> {
        match self {
            Self::SonarrRescan | Self::SonarrRename => Some(ArrService::Sonarr),
            Self::RadarrRescan | Self::RadarrRename => Some(ArrService::Radarr),
            _ => None,
        }
    }
}

pub fn resolve_operation(operation_id: Option<&str>, kind: &FlowBlockKind) -> BlockOperation {
    match operation_id.unwrap_or_else(|| kind.as_str()) {
        "operation.core.file-input" | "block.core.file-input" | "file-input" => {
            BlockOperation::Pass
        }
        "operation.core.file-output" | "block.core.file-output" | "file-output" => {
            BlockOperation::Stop
        }
        "operation.mediainfo.probe" | "block.media.inspect" | "media-inspect" => {
            BlockOperation::MediaInspect
        }
        "operation.core.condition" | "block.core.condition" | "condition" => {
            BlockOperation::Condition
        }
        "operation.mkvtoolnix.mkvpropedit" | "block.metadata.mkvpropedit" | "mkvpropedit" => {
            BlockOperation::Mkvpropedit
        }
        "operation.mkvtoolnix.mkvmerge.remux" | "block.video.remux-mkv" | "remux-mkv" => {
            BlockOperation::RemuxMkv
        }
        "operation.mp4box.remux" | "block.video.remux-mp4" | "remux-mp4" => {
            BlockOperation::RemuxMp4
        }
        "operation.ffmpeg.video.encode" | "block.video.encode" | "video-encode" => {
            BlockOperation::VideoEncode
        }
        "operation.ffmpeg.audio.encode" | "block.audio.encode" | "audio-encode" => {
            BlockOperation::AudioEncode
        }
        "operation.sonarr.rescan" | "block.sonarr.rescan" | "sonarr-rescan" => {
            BlockOperation::SonarrRescan
        }
        "operation.sonarr.rename" | "block.sonarr.rename" | "sonarr-rename" => {
            BlockOperation::SonarrRename
        }
        "operation.radarr.rescan" | "block.radarr.rescan" | "radarr-rescan" => {
            BlockOperation::RadarrRescan
        }
        "operation.radarr.rename" | "block.radarr.rename" | "radarr-rename" => {
            BlockOperation::RadarrRename
        }
        "operation.core.webhook" | "block.core.webhook" | "webhook" => BlockOperation::Webhook,
        _ => BlockOperation::Pass,
    }
}

pub fn decide_step(
    kind: &FlowBlockKind,
    config: &serde_json::Value,
    media_info: &serde_json::Value,
) -> StepDecision {
    decide_step_for_operation(None, kind, config, media_info)
}

pub fn decide_step_for_operation(
    operation_id: Option<&str>,
    kind: &FlowBlockKind,
    config: &serde_json::Value,
    media_info: &serde_json::Value,
) -> StepDecision {
    match resolve_operation(operation_id, kind) {
        BlockOperation::Pass => StepDecision::pass(),
        BlockOperation::Stop => StepDecision::stop(),
        BlockOperation::MediaInspect => StepDecision::reinpect(),
        BlockOperation::Condition => {
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
        BlockOperation::Mkvpropedit => StepDecision::edit(in_place_edit(config)),
        BlockOperation::RemuxMkv => StepDecision::transform(),
        BlockOperation::RemuxMp4 => StepDecision::remux_mp4(mp4_plan(config)),
        BlockOperation::VideoEncode => {
            StepDecision::encode(EncodePlan::Video(super::encode::video_plan(config)))
        }
        BlockOperation::AudioEncode => {
            StepDecision::encode(EncodePlan::Audio(super::encode::audio_plan(config)))
        }
        BlockOperation::SonarrRescan => arr_step(ArrService::Sonarr, ArrAction::Rescan, config),
        BlockOperation::SonarrRename => arr_step(ArrService::Sonarr, ArrAction::Rename, config),
        BlockOperation::RadarrRescan => arr_step(ArrService::Radarr, ArrAction::Rescan, config),
        BlockOperation::RadarrRename => arr_step(ArrService::Radarr, ArrAction::Rename, config),
        BlockOperation::Webhook => {
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
    fn each_block_kind_plans_a_single_effect() {
        let hevc = serde_json::json!({"tracks": [{"type": "video", "codec": "HEVC"}]});
        let empty = serde_json::json!({});

        for kind in ["file-input", "block.core.file-input"] {
            let parsed = FlowBlockKind::parse(kind);
            assert!(decide_step(&parsed, &empty, &hevc).port.is_none());
            assert!(!decide_step(&parsed, &empty, &hevc).stop);
        }

        let other = decide_step(&FlowBlockKind::parse("macro.unknown"), &empty, &hevc);
        assert!(
            !other.stop && !other.transform && other.encode.is_none() && other.notify.is_none()
        );

        assert!(decide_step(&FlowBlockKind::parse("file-output"), &empty, &hevc).stop);
        assert!(
            decide_step(
                &FlowBlockKind::parse("block.core.file-output"),
                &empty,
                &hevc
            )
            .stop
        );
        assert!(decide_step(&FlowBlockKind::parse("block.media.inspect"), &empty, &hevc).reinpect);
        let mp4 = decide_step(
            &FlowBlockKind::parse("block.video.remux-mp4"),
            &empty,
            &hevc,
        );
        assert!(mp4.encode.is_none() && !mp4.transform);
        assert!(mp4.remux_mp4.is_some_and(|plan| plan.faststart));

        let matched = decide_step(
            &FlowBlockKind::parse("block.core.condition"),
            &serde_json::json!({"property": "video.codec", "operator": "eq", "target": "hevc"}),
            &hevc,
        );
        assert_eq!(matched.port.as_deref(), Some("true"));

        let fallback = decide_step(
            &FlowBlockKind::parse("condition"),
            &serde_json::json!({}),
            &hevc,
        );
        assert_eq!(fallback.port.as_deref(), Some("true"));

        let edit = decide_step(
            &FlowBlockKind::parse("block.metadata.mkvpropedit"),
            &serde_json::json!({"default_audio_lang": "fre", "force_subtitles": true}),
            &hevc,
        );
        let edit = edit.edit.as_ref();
        assert_eq!(
            edit.and_then(|value| value.default_audio_lang.as_deref()),
            Some("fre")
        );
        assert_eq!(edit.map(|value| value.force_subtitles), Some(true));

        assert!(
            decide_step(
                &FlowBlockKind::parse("block.video.remux-mkv"),
                &empty,
                &hevc
            )
            .transform
        );
        let video = decide_step(&FlowBlockKind::parse("block.video.encode"), &empty, &hevc);
        assert!(matches!(video.encode, Some(EncodePlan::Video(_))));
        assert!(!video.transform);
        let audio = decide_step(
            &FlowBlockKind::parse("block.audio.encode"),
            &serde_json::json!({"codec": "aac", "normalize": true}),
            &hevc,
        );
        assert!(matches!(audio.encode, Some(EncodePlan::Audio(plan)) if plan.normalize));

        let notify = decide_step(
            &FlowBlockKind::parse("block.core.webhook"),
            &serde_json::json!({"url": "http://127.0.0.1/hook", "method": "PUT"}),
            &hevc,
        );
        assert_eq!(
            notify.notify.as_ref().map(|hook| hook.method.as_str()),
            Some("PUT")
        );
        assert!(
            decide_step(
                &FlowBlockKind::parse("webhook"),
                &serde_json::json!({"method": "POST"}),
                &hevc
            )
            .notify
            .is_none()
        );
    }

    #[test]
    fn arr_blocks_decide_an_integration_with_their_failure_policy() {
        let media = serde_json::json!({});
        let cases = [
            ("sonarr-rescan", ArrService::Sonarr, ArrAction::Rescan),
            ("block.sonarr.rescan", ArrService::Sonarr, ArrAction::Rescan),
            ("sonarr-rename", ArrService::Sonarr, ArrAction::Rename),
            ("block.sonarr.rename", ArrService::Sonarr, ArrAction::Rename),
            ("radarr-rescan", ArrService::Radarr, ArrAction::Rescan),
            ("block.radarr.rescan", ArrService::Radarr, ArrAction::Rescan),
            ("radarr-rename", ArrService::Radarr, ArrAction::Rename),
            ("block.radarr.rename", ArrService::Radarr, ArrAction::Rename),
        ];
        for (kind, service, action) in cases {
            let parsed = FlowBlockKind::parse(kind);
            let lenient = decide_step(&parsed, &serde_json::json!({}), &media);
            assert_eq!(
                lenient.integration,
                Some(ArrBlock {
                    service,
                    action,
                    fail_on_error: false
                })
            );
            assert!(!lenient.stop && lenient.notify.is_none());

            let strict = decide_step(&parsed, &serde_json::json!({"fail_on_error": true}), &media);
            assert_eq!(
                strict.integration,
                Some(ArrBlock {
                    service,
                    action,
                    fail_on_error: true
                })
            );
        }
        let other = decide_step(
            &FlowBlockKind::parse("block.core.file-output"),
            &serde_json::json!({}),
            &media,
        );
        assert!(other.integration.is_none());
    }
}
