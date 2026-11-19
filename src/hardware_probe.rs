//! Sondes matérielles légères d'un worker (ADR 0018) : `lspci`, `nvidia-smi` et `vainfo`.
//!
//! Elles ne lancent aucun encodage : elles lisent l'inventaire PCI, la génération NVIDIA et les
//! profils VA-API pour dériver les encodeurs matériels utilisables. Une sonde absente, lente ou en
//! échec ne déclare aucune capacité correspondante.

use forgearr_shared::capabilities::{
    EncodeAcceleration, HardwareEncoder, VideoCodec, WorkerCapabilities,
};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::OnceLock,
    time::{Duration, Instant},
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_POLL_INTERVAL: Duration = Duration::from_millis(50);
const RENDER_NODE_DIR: &str = "/dev/dri";
const RENDER_NODE_PREFIX: &str = "renderD";

/// Génération minimale (`majeur`, `mineur` de la capacité de calcul) pour chaque codec NVENC.
const NVENC_H264_MIN_COMPUTE: (u32, u32) = (3, 0);
const NVENC_HEVC_MIN_COMPUTE: (u32, u32) = (5, 2);
const NVENC_AV1_MIN_COMPUTE: (u32, u32) = (8, 9);

const GPU_PCI_CLASSES: [&str; 3] = [
    "VGA compatible controller",
    "3D controller",
    "Display controller",
];

/// Exécute une commande avec un délai maximal ; `None` si absente, en échec ou trop lente.
type ProbeRunner<'a> = &'a dyn Fn(&str, &[&str]) -> Option<String>;

fn run_probe(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < PROBE_TIMEOUT => {
                std::thread::sleep(PROBE_POLL_INTERVAL);
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    Some(output)
}

fn render_nodes() -> Vec<String> {
    let mut nodes: Vec<String> = std::fs::read_dir(RENDER_NODE_DIR)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(RENDER_NODE_PREFIX)
                .then(|| format!("{RENDER_NODE_DIR}/{name}"))
        })
        .collect();
    nodes.sort();
    nodes
}

/// Découpe un champ `lspci -mm` : le contenu des segments entre guillemets.
fn quoted_fields(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// Modèles de cartes graphiques d'une sortie `lspci -mm`.
pub fn parse_lspci_gpus(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let fields = quoted_fields(line);
            let [class, vendor, device, ..] = fields.as_slice() else {
                return None;
            };
            GPU_PCI_CLASSES
                .contains(class)
                .then(|| format!("{vendor} {device}"))
        })
        .collect()
}

/// Codecs encodables d'une sortie `vainfo` (points d'entrée `VAEntrypointEncSlice*`).
pub fn parse_vainfo_encoders(output: &str) -> Vec<HardwareEncoder> {
    output
        .lines()
        .filter_map(|line| {
            let (profile, entrypoint) = line.split_once(':')?;
            let profile = profile.trim().strip_prefix("VAProfile")?;
            if !entrypoint.trim().starts_with("VAEntrypointEncSlice") {
                return None;
            }
            let codec = if profile.starts_with("H264") {
                VideoCodec::H264
            } else if profile.starts_with("HEVC") {
                VideoCodec::Hevc
            } else if profile.starts_with("AV1") {
                VideoCodec::Av1
            } else {
                return None;
            };
            Some(HardwareEncoder {
                codec,
                acceleration: EncodeAcceleration::Vaapi,
            })
        })
        .collect()
}

fn parse_compute_capability(raw: &str) -> Option<(u32, u32)> {
    let (major, minor) = raw.trim().split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Encodeurs NVENC d'une sortie `nvidia-smi --query-gpu=compute_cap --format=csv,noheader`.
pub fn parse_nvidia_encoders(output: &str) -> Vec<HardwareEncoder> {
    let codecs = [
        (VideoCodec::H264, NVENC_H264_MIN_COMPUTE),
        (VideoCodec::Hevc, NVENC_HEVC_MIN_COMPUTE),
        (VideoCodec::Av1, NVENC_AV1_MIN_COMPUTE),
    ];
    output
        .lines()
        .filter_map(parse_compute_capability)
        .flat_map(|compute| {
            codecs
                .iter()
                .filter(move |(_, minimum)| compute >= *minimum)
                .map(|(codec, _)| HardwareEncoder {
                    codec: *codec,
                    acceleration: EncodeAcceleration::Nvenc,
                })
        })
        .collect()
}

/// Relève les capacités avec un exécuteur de commandes et des render nodes donnés.
pub fn detect_with(run: ProbeRunner<'_>, render_nodes: &[String]) -> WorkerCapabilities {
    let mut capabilities = WorkerCapabilities::default();
    if let Some(output) = run("lspci", &["-mm"]) {
        capabilities.gpu_models = parse_lspci_gpus(&output);
    }
    if let Some(output) = run(
        "nvidia-smi",
        &["--query-gpu=compute_cap", "--format=csv,noheader"],
    ) {
        capabilities
            .hardware_encoders
            .extend(parse_nvidia_encoders(&output));
    }
    for node in render_nodes {
        if let Some(output) = run("vainfo", &["--display", "drm", "--device", node]) {
            capabilities
                .hardware_encoders
                .extend(parse_vainfo_encoders(&output));
        }
    }
    capabilities.normalize();
    capabilities
}

/// Capacités de cette machine, relevées une seule fois (le matériel ne change pas en cours de vie).
pub fn detected_capabilities() -> WorkerCapabilities {
    static CAPABILITIES: OnceLock<WorkerCapabilities> = OnceLock::new();
    CAPABILITIES
        .get_or_init(|| detect_with(&run_probe, &render_nodes()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSPCI: &str = "\
00:02.0 \"VGA compatible controller\" \"Intel Corporation\" \"Alder Lake-P GT2 [Iris Xe Graphics]\" -r0c -p00 \"Dell\" \"Device 0a5f\"
00:1f.3 \"Audio device\" \"Intel Corporation\" \"Alder Lake PCH-P High Definition Audio\" -r01 -p80 \"Dell\" \"Device 0a5f\"
01:00.0 \"3D controller\" \"NVIDIA Corporation\" \"AD107M [GeForce RTX 4060 Max-Q]\" -ra1 \"Dell\" \"Device 0b00\"
";

    const VAINFO: &str = "\
vainfo: VA-API version: 1.17 (libva 2.12.0)
      VAProfileH264Main               : VAEntrypointVLD
      VAProfileH264Main               : VAEntrypointEncSlice
      VAProfileHEVCMain               : VAEntrypointEncSliceLP
      VAProfileHEVCMain10             : VAEntrypointEncSliceLP
      VAProfileAV1Profile0            : VAEntrypointVLD
      VAProfileJPEGBaseline           : VAEntrypointEncPicture
";

    fn encoder(codec: VideoCodec, acceleration: EncodeAcceleration) -> HardwareEncoder {
        HardwareEncoder {
            codec,
            acceleration,
        }
    }

    #[test]
    fn lspci_keeps_only_graphics_controllers() {
        assert_eq!(
            parse_lspci_gpus(LSPCI),
            [
                "Intel Corporation Alder Lake-P GT2 [Iris Xe Graphics]",
                "NVIDIA Corporation AD107M [GeForce RTX 4060 Max-Q]",
            ]
        );
        assert!(parse_lspci_gpus("garbage").is_empty());
    }

    #[test]
    fn vainfo_reports_only_encode_entrypoints_of_known_codecs() {
        let mut found = parse_vainfo_encoders(VAINFO);
        found.sort();
        found.dedup();
        assert_eq!(
            found,
            [
                encoder(VideoCodec::H264, EncodeAcceleration::Vaapi),
                encoder(VideoCodec::Hevc, EncodeAcceleration::Vaapi),
            ]
        );
    }

    #[test]
    fn nvidia_generation_decides_the_codecs() {
        let kepler = parse_nvidia_encoders("3.5\n");
        assert_eq!(
            kepler,
            [encoder(VideoCodec::H264, EncodeAcceleration::Nvenc)]
        );
        let pascal = parse_nvidia_encoders("6.1\n");
        assert_eq!(pascal.len(), 2);
        let ada = parse_nvidia_encoders("8.9\n");
        assert_eq!(ada.len(), 3);
        assert!(parse_nvidia_encoders("not a number\n").is_empty());
    }

    #[test]
    fn detect_with_merges_probes_and_ignores_missing_tools() {
        let run = |program: &str, _args: &[&str]| match program {
            "lspci" => Some(LSPCI.to_owned()),
            "vainfo" => Some(VAINFO.to_owned()),
            _ => None,
        };
        let nodes = ["/dev/dri/renderD128".to_owned()];
        let capabilities = detect_with(&run, &nodes);
        assert_eq!(capabilities.gpu_models.len(), 2);
        assert!(capabilities.can_encode_in_hardware(VideoCodec::Hevc));
        assert!(!capabilities.can_encode_in_hardware(VideoCodec::Av1));
    }

    #[test]
    fn detect_with_declares_nothing_when_every_probe_fails() {
        let run = |_: &str, _: &[&str]| None;
        assert_eq!(
            detect_with(&run, &["/dev/dri/renderD128".to_owned()]),
            WorkerCapabilities::default()
        );
    }
}
