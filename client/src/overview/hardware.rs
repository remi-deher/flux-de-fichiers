//! Utilitaires de formatage matériel (CPU, RAM, disque, GPU) partagés par les vues workers.

use crate::ui::MetricTone;
use flux_shared::format::format_bytes;

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

pub fn used_label(used: u64, total: u64) -> String {
    format!(
        "{} utilisés sur {}",
        format_bytes(used),
        format_bytes(total)
    )
}

pub fn gpu_label(devices: &[String]) -> String {
    match devices.len() {
        0 => "Aucun GPU détecté".to_owned(),
        1 => "1 périphérique GPU".to_owned(),
        count => format!("{count} périphériques GPU"),
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
