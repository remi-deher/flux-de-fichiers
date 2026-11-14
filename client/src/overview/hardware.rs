//! Charge matérielle du central : processeur, mémoire, disque et GPU, relevés à l'ouverture.

use crate::api;
use crate::ui::{IconName, MetricCard, MetricTone};
use flux_shared::format::format_bytes;
use flux_shared::system::{HardwareStatus, VolumeSpace};
use leptos::prelude::*;

const HARDWARE_PATH: &str = "/api/v1/system/hardware";
const FULL_PERCENT: f32 = 100.0;
const WARNING_PERCENT: f32 = 80.0;
const DANGER_PERCENT: f32 = 95.0;

/// Part utilisée en pourcentage ; 0 quand la capacité est inconnue.
pub fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f32 / total as f32 * FULL_PERCENT).min(FULL_PERCENT)
    }
}

/// Ton d'une jauge selon son taux d'occupation.
pub fn usage_tone(percent: f32) -> MetricTone {
    if percent >= DANGER_PERCENT {
        MetricTone::Danger
    } else if percent >= WARNING_PERCENT {
        MetricTone::Warning
    } else {
        MetricTone::Neutral
    }
}

fn used_label(used: u64, total: u64) -> String {
    format!(
        "{} utilisés sur {}",
        format_bytes(used),
        format_bytes(total)
    )
}

fn gpu_label(devices: &[String]) -> String {
    match devices.len() {
        0 => "Aucun GPU détecté".to_owned(),
        1 => "1 périphérique GPU".to_owned(),
        count => format!("{count} périphériques GPU"),
    }
}

fn tile(label: &'static str, percent: f32, subtitle: String, icon: IconName) -> impl IntoView {
    view! {
        <MetricCard label=label value=format!("{percent:.0} %") subtitle=subtitle icon=icon tone=usage_tone(percent) />
    }
}

fn tiles(hardware: &HardwareStatus) -> impl IntoView + use<> {
    let memory = percent(hardware.ram_used_bytes, hardware.ram_total_bytes);
    let disk = hardware.volume.map(
        |VolumeSpace {
             total_bytes,
             available_bytes,
         }| {
            let used = total_bytes.saturating_sub(available_bytes);
            (percent(used, total_bytes), used_label(used, total_bytes))
        },
    );
    view! {
        {tile("Processeur", hardware.cpu_usage_percent, format!("{} cœurs logiques, charge sur 1 min", hardware.logical_cores), IconName::Activity)}
        {tile("Mémoire", memory, used_label(hardware.ram_used_bytes, hardware.ram_total_bytes), IconName::Workers)}
        {disk.map(|(percent, subtitle)| tile("Disque des données", percent, subtitle, IconName::Libraries))}
        <MetricCard label="GPU" value=hardware.gpu_devices.len().to_string() subtitle=gpu_label(&hardware.gpu_devices) icon=IconName::Refresh />
    }
}

#[component]
pub fn HardwareWidget() -> impl IntoView {
    let hardware = LocalResource::new(|| api::get::<HardwareStatus>(HARDWARE_PATH));
    view! {
        <section class="metrics-grid" aria-label="Matériel du central">
            {move || match hardware.get() {
                None => view! { <p class="text-dim">"Relevé du matériel…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                Some(Ok(hardware)) => tiles(&hardware).into_any(),
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_bounded_and_zero_without_capacity() {
        assert_eq!(percent(1, 4), 25.0);
        assert_eq!(percent(9, 4), FULL_PERCENT);
        assert_eq!(percent(5, 0), 0.0);
    }

    #[test]
    fn tone_follows_the_warning_and_danger_thresholds() {
        assert_eq!(usage_tone(10.0), MetricTone::Neutral);
        assert_eq!(usage_tone(WARNING_PERCENT), MetricTone::Warning);
        assert_eq!(usage_tone(DANGER_PERCENT), MetricTone::Danger);
    }

    #[test]
    fn gpu_label_counts_devices() {
        assert_eq!(gpu_label(&[]), "Aucun GPU détecté");
        assert_eq!(gpu_label(&["a".into()]), "1 périphérique GPU");
        assert_eq!(gpu_label(&["a".into(), "b".into()]), "2 périphériques GPU");
    }
}
