//! Construction des argv FFmpeg pour les blocs d'encodage issus des plugins (ADR 0006, 0012).

use serde_json::Value;
use std::path::Path;

/// Plan d'encodage prêt à être lancé par le moteur de flux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodePlan {
    Video(VideoEncodePlan),
    Audio(AudioEncodePlan),
}

/// Valeurs de `encoder` qui demandent une accélération matérielle (repli CPU possible).
const HARDWARE_ENCODERS: [&str; 4] = ["nvidia", "intel_qsv", "vaapi", "amd_amf"];
/// Valeur de `encoder` qui force l'encodage logiciel.
const CPU_ENCODER: &str = "cpu";

impl EncodePlan {
    /// Même plan en encodage logiciel, si celui-ci demande un encodeur matériel.
    pub fn cpu_fallback(&self) -> Option<Self> {
        match self {
            Self::Video(plan) if HARDWARE_ENCODERS.contains(&plan.encoder.as_str()) => {
                Some(Self::Video(VideoEncodePlan {
                    encoder: CPU_ENCODER.to_owned(),
                    ..plan.clone()
                }))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoEncodePlan {
    pub codec: String,
    pub encoder: String,
    pub quality: u8,
    pub speed: u8,
    pub custom_parameters: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioEncodePlan {
    pub codec: String,
    pub bitrate: String,
    pub channels: String,
    pub normalize: bool,
    pub custom_parameters: Vec<String>,
}

pub fn video_plan(config: &Value) -> VideoEncodePlan {
    VideoEncodePlan {
        codec: text(config, "codec", "h265"),
        encoder: text(config, "encoder", "auto"),
        quality: number(config, "quality", 6).clamp(1, 9),
        speed: number(config, "speed", 3).clamp(1, 5),
        custom_parameters: split_custom(config),
    }
}

pub fn audio_plan(config: &Value) -> AudioEncodePlan {
    AudioEncodePlan {
        codec: text(config, "codec", "aac"),
        bitrate: text(config, "bitrate", "192"),
        channels: text(config, "channels", "preserve"),
        normalize: config
            .get("normalize")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        custom_parameters: split_custom(config),
    }
}

fn text(config: &Value, key: &str, default: &str) -> String {
    config
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
        .to_owned()
}

fn number(config: &Value, key: &str, default: u8) -> u8 {
    match config.get(key) {
        Some(Value::Number(number)) => number.as_u64().unwrap_or(u64::from(default)) as u8,
        Some(Value::String(text)) => text.parse().unwrap_or(default),
        _ => default,
    }
}

pub(super) fn split_custom(config: &Value) -> Vec<String> {
    config
        .get("custom_parameters")
        .and_then(Value::as_str)
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Mappe qualité 1..=9 vers une échelle CRF/CQ (plus bas = meilleur).
pub fn quality_to_crf(quality: u8) -> u8 {
    match quality.clamp(1, 9) {
        1 => 32,
        2 => 30,
        3 => 28,
        4 => 26,
        5 => 24,
        6 => 22,
        7 => 20,
        8 => 18,
        _ => 16,
    }
}

/// Mappe vitesse 1..=5 vers un préréglage FFmpeg CPU.
pub fn speed_to_preset(speed: u8) -> &'static str {
    match speed.clamp(1, 5) {
        1 => "veryslow",
        2 => "slow",
        3 => "medium",
        4 => "fast",
        _ => "veryfast",
    }
}

/// Encodeur FFmpeg vidéo d'après codec + accélération demandée.
pub fn video_encoder_name(codec: &str, encoder: &str) -> &'static str {
    let hardware = match encoder {
        "auto" | "cpu" => "cpu",
        "nvidia" => "nvidia",
        "intel_qsv" => "qsv",
        "vaapi" => "vaapi",
        "amd_amf" => "amf",
        _ => "cpu",
    };
    match (codec, hardware) {
        ("h264", "nvidia") => "h264_nvenc",
        ("h264", "qsv") => "h264_qsv",
        ("h264", "vaapi") => "h264_vaapi",
        ("h264", "amf") => "h264_amf",
        ("h264", _) => "libx264",
        ("h265" | "h265_10bit", "nvidia") => "hevc_nvenc",
        ("h265" | "h265_10bit", "qsv") => "hevc_qsv",
        ("h265" | "h265_10bit", "vaapi") => "hevc_vaapi",
        ("h265" | "h265_10bit", "amf") => "hevc_amf",
        ("h265" | "h265_10bit", _) => "libx265",
        ("av1" | "av1_10bit", "nvidia") => "av1_nvenc",
        ("av1" | "av1_10bit", "qsv") => "av1_qsv",
        ("av1" | "av1_10bit", _) => "libsvtav1",
        ("vp9", _) => "libvpx-vp9",
        _ => "libx265",
    }
}

pub fn audio_encoder_name(codec: &str) -> &'static str {
    match codec {
        "aac" => "aac",
        "eac3" => "eac3",
        "ac3" => "ac3",
        "flac" => "flac",
        "mp3" => "libmp3lame",
        "opus" => "libopus",
        _ => "aac",
    }
}

/// Argv FFmpeg (sans l'exécutable) pour un encodage vidéo : copie audio/sous-titres.
pub fn video_argv(plan: &VideoEncodePlan, source: &Path, output: &Path) -> Vec<String> {
    let encoder = video_encoder_name(&plan.codec, &plan.encoder);
    let crf = quality_to_crf(plan.quality).to_string();
    let preset = speed_to_preset(plan.speed);
    let mut args = vec![
        "-nostdin".into(),
        "-y".into(),
        "-hide_banner".into(),
        "-i".into(),
        source.display().to_string(),
        "-map".into(),
        "0".into(),
        "-c:v".into(),
        encoder.to_owned(),
    ];
    match plan.encoder.as_str() {
        "nvidia" => {
            args.push("-cq".into());
            args.push(crf);
            args.push("-preset".into());
            args.push(match plan.speed.clamp(1, 5) {
                1 => "p7".into(),
                2 => "p6".into(),
                3 => "p4".into(),
                4 => "p2".into(),
                _ => "p1".into(),
            });
        }
        "intel_qsv" => {
            args.push("-global_quality".into());
            args.push(crf);
        }
        "vaapi" => {
            args.push("-qp".into());
            args.push(crf);
        }
        _ => {
            args.push("-crf".into());
            args.push(crf);
            if encoder != "libsvtav1" {
                args.push("-preset".into());
                args.push(preset.to_owned());
            } else {
                args.push("-preset".into());
                args.push(
                    match plan.speed.clamp(1, 5) {
                        1 => "4",
                        2 => "6",
                        3 => "8",
                        4 => "10",
                        _ => "12",
                    }
                    .into(),
                );
            }
        }
    }
    if plan.codec.ends_with("_10bit") && !matches!(plan.encoder.as_str(), "vaapi") {
        args.push("-pix_fmt".into());
        args.push(if matches!(plan.encoder.as_str(), "nvidia" | "intel_qsv") {
            "p010le".into()
        } else {
            "yuv420p10le".into()
        });
    }
    args.push("-c:a".into());
    args.push("copy".into());
    args.push("-c:s".into());
    args.push("copy".into());
    args.extend(plan.custom_parameters.iter().cloned());
    args.push(output.display().to_string());
    args
}

/// Argv FFmpeg pour un encodage audio : copie vidéo/sous-titres, transcode audio.
pub fn audio_argv(plan: &AudioEncodePlan, source: &Path, output: &Path) -> Vec<String> {
    let encoder = audio_encoder_name(&plan.codec);
    let mut args = vec![
        "-nostdin".into(),
        "-y".into(),
        "-hide_banner".into(),
        "-i".into(),
        source.display().to_string(),
        "-map".into(),
        "0".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:s".into(),
        "copy".into(),
        "-c:a".into(),
        encoder.to_owned(),
    ];
    if plan.codec != "flac" {
        args.push("-b:a".into());
        args.push(format!("{}k", plan.bitrate));
    }
    match plan.channels.as_str() {
        "stereo" => {
            args.push("-ac".into());
            args.push("2".into());
        }
        "5.1" => {
            args.push("-ac".into());
            args.push("6".into());
        }
        _ => {}
    }
    if plan.normalize {
        args.push("-af".into());
        args.push("loudnorm".into());
    }
    args.extend(plan.custom_parameters.iter().cloned());
    args.push(output.display().to_string());
    args
}

pub fn argv_for(plan: &EncodePlan, source: &Path, output: &Path) -> Vec<String> {
    match plan {
        EncodePlan::Video(plan) => video_argv(plan, source, output),
        EncodePlan::Audio(plan) => audio_argv(plan, source, output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn video_plan_reads_plugin_fields_and_builds_cpu_hevc_argv() {
        let plan = video_plan(&json!({
            "codec": "h265",
            "encoder": "cpu",
            "quality": "7",
            "speed": "2",
            "custom_parameters": "-tune grain"
        }));
        assert_eq!(plan.quality, 7);
        assert_eq!(video_encoder_name(&plan.codec, &plan.encoder), "libx265");
        let args = video_argv(&plan, Path::new("in.mkv"), Path::new("out.mkv"));
        assert!(args.windows(2).any(|pair| pair == ["-c:v", "libx265"]));
        assert!(args.windows(2).any(|pair| pair == ["-crf", "20"]));
        assert!(args.windows(2).any(|pair| pair == ["-preset", "slow"]));
        assert!(args.iter().any(|arg| arg == "-tune"));
        assert_eq!(args.last().map(String::as_str), Some("out.mkv"));
    }

    #[test]
    fn audio_plan_builds_aac_with_loudnorm() {
        let plan = audio_plan(&json!({
            "codec": "aac",
            "bitrate": "256",
            "channels": "stereo",
            "normalize": true
        }));
        let args = audio_argv(&plan, &PathBuf::from("in.mkv"), &PathBuf::from("out.mkv"));
        assert!(args.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(args.windows(2).any(|pair| pair == ["-b:a", "256k"]));
        assert!(args.windows(2).any(|pair| pair == ["-ac", "2"]));
        assert!(args.windows(2).any(|pair| pair == ["-af", "loudnorm"]));
    }

    #[test]
    fn only_hardware_video_plans_have_a_cpu_fallback() {
        for hardware in HARDWARE_ENCODERS {
            let plan = EncodePlan::Video(video_plan(&json!({"encoder": hardware})));
            let Some(EncodePlan::Video(cpu)) = plan.cpu_fallback() else {
                panic!("repli attendu pour {hardware}");
            };
            assert_eq!(cpu.encoder, "cpu");
            assert_eq!(cpu.quality, 6);
        }
        for software in ["auto", "cpu"] {
            let plan = EncodePlan::Video(video_plan(&json!({"encoder": software})));
            assert!(plan.cpu_fallback().is_none());
        }
        assert!(
            EncodePlan::Audio(audio_plan(&json!({})))
                .cpu_fallback()
                .is_none()
        );
    }

    #[test]
    fn auto_encoder_falls_back_to_cpu_lib() {
        assert_eq!(video_encoder_name("h264", "auto"), "libx264");
        assert_eq!(video_encoder_name("av1", "nvidia"), "av1_nvenc");
    }
}
