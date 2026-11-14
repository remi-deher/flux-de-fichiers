//! Construction de l'argv MP4Box pour le bloc `block.video.remux-mp4` (ADR 0006, 0012).

use serde_json::Value;
use std::path::Path;

/// Entrelacement (ms) qui place l'atome `moov` en tête de fichier (lecture immédiate).
const FASTSTART_INTERLEAVE_MS: u32 = 500;

/// Plan de remuxage MP4 prêt à être lancé par le moteur de flux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mp4Plan {
    pub faststart: bool,
    pub custom_parameters: Vec<String>,
}

pub fn mp4_plan(config: &Value) -> Mp4Plan {
    Mp4Plan {
        faststart: config
            .get("faststart")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        custom_parameters: super::encode::split_custom(config),
    }
}

/// Argv MP4Box (sans l'exécutable) : importe la source dans un nouveau MP4, sans réencodage.
pub fn mp4box_argv(plan: &Mp4Plan, source: &Path, output: &Path) -> Vec<String> {
    let mut args = vec!["-add".to_owned(), source.display().to_string()];
    if plan.faststart {
        args.push("-inter".into());
        args.push(FASTSTART_INTERLEAVE_MS.to_string());
    } else {
        args.push("-flat".into());
    }
    args.extend(plan.custom_parameters.iter().cloned());
    args.push("-new".into());
    args.push(output.display().to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plan_defaults_to_faststart_and_reads_custom_parameters() {
        assert!(mp4_plan(&json!({})).faststart);
        let plan = mp4_plan(&json!({"faststart": false, "custom_parameters": "-brand mp42"}));
        assert!(!plan.faststart);
        assert_eq!(plan.custom_parameters, ["-brand", "mp42"]);
    }

    #[test]
    fn argv_places_moov_first_or_flat() {
        let source = Path::new("in.mkv");
        let output = Path::new("out.mp4");
        let fast = mp4box_argv(&mp4_plan(&json!({})), source, output);
        assert_eq!(fast, ["-add", "in.mkv", "-inter", "500", "-new", "out.mp4"]);
        let flat = mp4box_argv(
            &mp4_plan(&json!({"faststart": false, "custom_parameters": "-lang fr"})),
            source,
            output,
        );
        assert_eq!(
            flat,
            ["-add", "in.mkv", "-flat", "-lang", "fr", "-new", "out.mp4"]
        );
    }
}
