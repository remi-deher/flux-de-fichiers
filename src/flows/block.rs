use super::{
    arr::{ArrAction, ArrBlock, ArrService},
    condition::{ConditionConfig, evaluate_condition},
    encode::EncodePlan,
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

pub fn decide_step(
    kind: &FlowBlockKind,
    config: &serde_json::Value,
    media_info: &serde_json::Value,
) -> StepDecision {
    match kind {
        FlowBlockKind::FileInput | FlowBlockKind::Other(_) => StepDecision::pass(),
        FlowBlockKind::FileOutput => StepDecision::stop(),
        FlowBlockKind::MediaInspect => StepDecision::reinpect(),
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
        FlowBlockKind::RemuxMkv => StepDecision::transform(),
        // MP4 via MP4Box : pas encore d'exécuteur dédié — traverse sans effet.
        FlowBlockKind::RemuxMp4 => StepDecision::pass(),
        FlowBlockKind::VideoEncode => {
            StepDecision::encode(EncodePlan::Video(super::encode::video_plan(config)))
        }
        FlowBlockKind::AudioEncode => {
            StepDecision::encode(EncodePlan::Audio(super::encode::audio_plan(config)))
        }
        FlowBlockKind::SonarrRescan => arr_step(ArrService::Sonarr, ArrAction::Rescan, config),
        FlowBlockKind::SonarrRename => arr_step(ArrService::Sonarr, ArrAction::Rename, config),
        FlowBlockKind::RadarrRescan => arr_step(ArrService::Radarr, ArrAction::Rescan, config),
        FlowBlockKind::RadarrRename => arr_step(ArrService::Radarr, ArrAction::Rename, config),
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
    fn each_block_kind_plans_a_single_effect() {
        let hevc = serde_json::json!({"tracks": [{"type": "video", "codec": "HEVC"}]});
        let empty = serde_json::json!({});

        assert!(
            decide_step(&FlowBlockKind::FileInput, &empty, &hevc)
                .port
                .is_none()
        );
        assert!(!decide_step(&FlowBlockKind::FileInput, &empty, &hevc).stop);

        let other = decide_step(&FlowBlockKind::Other("macro.unknown".into()), &empty, &hevc);
        assert!(
            !other.stop && !other.transform && other.encode.is_none() && other.notify.is_none()
        );

        assert!(decide_step(&FlowBlockKind::FileOutput, &empty, &hevc).stop);
        assert!(decide_step(&FlowBlockKind::MediaInspect, &empty, &hevc).reinpect);
        assert!(
            decide_step(&FlowBlockKind::RemuxMp4, &empty, &hevc)
                .encode
                .is_none()
        );

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
        let video = decide_step(&FlowBlockKind::VideoEncode, &empty, &hevc);
        assert!(matches!(video.encode, Some(EncodePlan::Video(_))));
        assert!(!video.transform);
        let audio = decide_step(
            &FlowBlockKind::AudioEncode,
            &serde_json::json!({"codec": "aac", "normalize": true}),
            &hevc,
        );
        assert!(matches!(audio.encode, Some(EncodePlan::Audio(plan)) if plan.normalize));

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

    #[test]
    fn arr_blocks_decide_an_integration_with_their_failure_policy() {
        let media = serde_json::json!({});
        let cases = [
            (
                FlowBlockKind::SonarrRescan,
                ArrService::Sonarr,
                ArrAction::Rescan,
            ),
            (
                FlowBlockKind::SonarrRename,
                ArrService::Sonarr,
                ArrAction::Rename,
            ),
            (
                FlowBlockKind::RadarrRescan,
                ArrService::Radarr,
                ArrAction::Rescan,
            ),
            (
                FlowBlockKind::RadarrRename,
                ArrService::Radarr,
                ArrAction::Rename,
            ),
        ];
        for (kind, service, action) in cases {
            let lenient = decide_step(&kind, &serde_json::json!({}), &media);
            assert_eq!(
                lenient.integration,
                Some(ArrBlock {
                    service,
                    action,
                    fail_on_error: false
                })
            );
            assert!(!lenient.stop && lenient.notify.is_none());

            let strict = decide_step(&kind, &serde_json::json!({"fail_on_error": true}), &media);
            assert_eq!(
                strict.integration,
                Some(ArrBlock {
                    service,
                    action,
                    fail_on_error: true
                })
            );
        }
        let other = decide_step(&FlowBlockKind::FileOutput, &serde_json::json!({}), &media);
        assert!(other.integration.is_none());
    }
}
