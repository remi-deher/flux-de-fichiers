use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_CORES: u32 = 1;
const KILOBYTES_TO_BYTES: u64 = 1024;

/// Instantané des métriques matérielles et système d'un nœud.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TelemetrySnapshot {
    pub timestamp: DateTime<Utc>,
    pub logical_cores: u32,
    pub cpu_usage_percent: f32,
    pub load_average: [f32; 3],
    pub ram_total_bytes: u64,
    pub ram_available_bytes: u64,
    pub ram_used_bytes: u64,
    pub gpu_detected: bool,
    pub gpu_devices: Vec<String>,
    pub storage_total_bytes: u64,
    pub storage_available_bytes: u64,
}

/// Sonde système in-process interrogeant `/proc`, `/sys`, `/dev/dri` et `statvfs`.
#[derive(Clone, Debug)]
pub struct TelemetryProbe {
    storage_path: PathBuf,
}

impl Default for TelemetryProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryProbe {
    pub fn new() -> Self {
        let storage_path = std::env::var("WORK_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        Self { storage_path }
    }

    pub fn with_storage_path(storage_path: impl Into<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.into(),
        }
    }

    /// Échantillonne l'état système à l'instant T sans élever de privilèges.
    pub fn sample(&self) -> TelemetrySnapshot {
        let logical_cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(DEFAULT_CORES);

        let load_average = Self::read_load_average().unwrap_or([0.0, 0.0, 0.0]);
        let cpu_usage_percent = Self::estimate_cpu_usage(load_average, logical_cores);

        let (ram_total_bytes, ram_available_bytes, ram_used_bytes) =
            Self::read_memory_info().unwrap_or((0, 0, 0));

        let gpu_devices = Self::detect_gpu_devices();
        let gpu_detected = !gpu_devices.is_empty();

        let (storage_total_bytes, storage_available_bytes) =
            Self::read_storage_info(&self.storage_path).unwrap_or((0, 0));

        TelemetrySnapshot {
            timestamp: Utc::now(),
            logical_cores,
            cpu_usage_percent,
            load_average,
            ram_total_bytes,
            ram_available_bytes,
            ram_used_bytes,
            gpu_detected,
            gpu_devices,
            storage_total_bytes,
            storage_available_bytes,
        }
    }

    /// Lit les moyennes de charge système depuis `/proc/loadavg`.
    fn read_load_average() -> Option<[f32; 3]> {
        let content = fs::read_to_string("/proc/loadavg").ok()?;
        let mut parts = content.split_whitespace();
        let l1 = parts.next()?.parse::<f32>().ok()?;
        let l5 = parts.next()?.parse::<f32>().ok()?;
        let l15 = parts.next()?.parse::<f32>().ok()?;
        Some([l1, l5, l15])
    }

    /// Estime le pourcentage CPU instantané à partir du loadavg 1m et des cœurs logiques.
    fn estimate_cpu_usage(load_avg: [f32; 3], cores: u32) -> f32 {
        if cores == 0 {
            return 0.0;
        }
        let normalized = (load_avg[0] / cores as f32) * 100.0;
        normalized.clamp(0.0, 100.0)
    }

    /// Lit la mémoire vive totale et disponible depuis `/proc/meminfo`.
    fn read_memory_info() -> Option<(u64, u64, u64)> {
        let content = fs::read_to_string("/proc/meminfo").ok()?;
        let mut total_kb: Option<u64> = None;
        let mut avail_kb: Option<u64> = None;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = Self::parse_meminfo_line(line);
            } else if line.starts_with("MemAvailable:") {
                avail_kb = Self::parse_meminfo_line(line);
            }
            if total_kb.is_some() && avail_kb.is_some() {
                break;
            }
        }

        let total = total_kb? * KILOBYTES_TO_BYTES;
        let available = avail_kb.unwrap_or(0) * KILOBYTES_TO_BYTES;
        let used = total.saturating_sub(available);
        Some((total, available, used))
    }

    fn parse_meminfo_line(line: &str) -> Option<u64> {
        let mut parts = line.split_whitespace();
        parts.next()?; // e.g. "MemTotal:"
        parts.next()?.parse::<u64>().ok()
    }

    /// Détecte les périphériques GPU disponibles sans privilèges root.
    fn detect_gpu_devices() -> Vec<String> {
        let mut devices = Vec::new();

        // 1. Détection des render nodes DRM Linux standard (/dev/dri/renderD*)
        if let Ok(entries) = fs::read_dir("/dev/dri") {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if name.starts_with("renderD") || name.starts_with("card") {
                    devices.push(format!("/dev/dri/{name}"));
                }
            }
        }

        // 2. Détection NVIDIA si présente
        if Path::new("/dev/nvidia0").exists() {
            devices.push("/dev/nvidia0".to_string());
        }

        devices.sort();
        devices.dedup();
        devices
    }

    /// Lit l'espace total et disponible via l'appel POSIX `statvfs`.
    fn read_storage_info(path: &Path) -> Option<(u64, u64)> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();

        let ret = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
        if ret != 0 {
            return None;
        }

        let stat = unsafe { stat.assume_init() };
        #[allow(clippy::unnecessary_cast)]
        let fragment_size = stat.f_frsize as u64;
        #[allow(clippy::unnecessary_cast)]
        let total = (stat.f_blocks as u64).saturating_mul(fragment_size);
        #[allow(clippy::unnecessary_cast)]
        let available = (stat.f_bavail as u64).saturating_mul(fragment_size);

        Some((total, available))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_probe_sample_basic() {
        let probe = TelemetryProbe::with_storage_path(".");
        let snapshot = probe.sample();

        assert!(snapshot.logical_cores >= 1);
        assert!(snapshot.cpu_usage_percent >= 0.0 && snapshot.cpu_usage_percent <= 100.0);
        assert_eq!(snapshot.load_average.len(), 3);
    }

    #[test]
    fn test_estimate_cpu_usage() {
        assert_eq!(TelemetryProbe::estimate_cpu_usage([0.0, 0.0, 0.0], 4), 0.0);
        assert_eq!(TelemetryProbe::estimate_cpu_usage([2.0, 1.0, 0.5], 4), 50.0);
        assert_eq!(
            TelemetryProbe::estimate_cpu_usage([8.0, 4.0, 2.0], 4),
            100.0
        );
    }

    #[test]
    fn test_parse_meminfo_line() {
        let line = "MemTotal:       32644268 kB";
        assert_eq!(TelemetryProbe::parse_meminfo_line(line), Some(32_644_268));

        let invalid = "InvalidLine";
        assert_eq!(TelemetryProbe::parse_meminfo_line(invalid), None);
    }
}
