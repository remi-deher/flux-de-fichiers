//! Fuseaux horaires IANA, détection heure d'été / heure d'hiver (DST), UTC et GMT.
//!
//! Partagé entre le central (plannings horaires des workers, API système) et le client WASM
//! (formatage cohérent des dates et journaux).

use chrono::{DateTime, Datelike, Offset, TimeZone, Timelike, Utc};
use chrono_tz::{OffsetComponents, Tz};
use serde::{Deserialize, Serialize};

/// Fuseau par défaut en l'absence de variable `TZ` ou de réglage en base.
pub const DEFAULT_TIMEZONE: &str = "UTC";
const SECONDS_PER_MINUTE: i32 = 60;
const SECONDS_PER_HOUR: i32 = 3_600;

/// Régime horaire actif à un instant donné pour un fuseau horaire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DstState {
    /// Temps universel ou décalage fixe (UTC, GMT, Etc/GMT±N), sans changement d'heure.
    Universal,
    /// Heure d'hiver (heure standard d'un fuseau sujet à changement d'heure).
    Winter,
    /// Heure d'été (DST actif).
    Summer,
}

impl DstState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Universal => "Temps universel / fixe (sans changement d'heure)",
            Self::Winter => "Heure d'hiver (standard)",
            Self::Summer => "Heure d'été (DST)",
        }
    }
}

/// Groupe d'affichage dans le sélecteur de fuseaux horaires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimezoneGroup {
    Universal,
    Europe,
    Americas,
    Overseas,
}

impl TimezoneGroup {
    pub const ALL: [Self; 4] = [
        Self::Universal,
        Self::Europe,
        Self::Americas,
        Self::Overseas,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Universal => "Temps universel (UTC / GMT / fixe)",
            Self::Europe => "Europe (heure d'été / hiver auto)",
            Self::Americas => "Amériques (heure d'été / hiver auto)",
            Self::Overseas => "Outre-mer & Pacifique",
        }
    }
}

/// Fuseau horaire pré-proposé dans l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimezonePreset {
    pub id: &'static str,
    pub label: &'static str,
    pub group: TimezoneGroup,
}

/// Fuseaux horaires courants couvrant UTC, GMT, décalages fixes et zones DST principales.
pub const TIMEZONE_PRESETS: &[TimezonePreset] = &[
    TimezonePreset {
        id: "UTC",
        label: "UTC (Temps universel coordonné)",
        group: TimezoneGroup::Universal,
    },
    TimezonePreset {
        id: "Etc/GMT",
        label: "GMT (Greenwich Mean Time, UTC+00:00)",
        group: TimezoneGroup::Universal,
    },
    TimezonePreset {
        id: "Etc/GMT-1",
        label: "UTC+01:00 fixe (heure d'hiver permanente)",
        group: TimezoneGroup::Universal,
    },
    TimezonePreset {
        id: "Etc/GMT-2",
        label: "UTC+02:00 fixe (heure d'été permanente)",
        group: TimezoneGroup::Universal,
    },
    TimezonePreset {
        id: "Etc/GMT+4",
        label: "UTC-04:00 fixe",
        group: TimezoneGroup::Universal,
    },
    TimezonePreset {
        id: "Etc/GMT+5",
        label: "UTC-05:00 fixe",
        group: TimezoneGroup::Universal,
    },
    TimezonePreset {
        id: "Europe/Paris",
        label: "Europe/Paris (France — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Brussels",
        label: "Europe/Brussels (Belgique — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Zurich",
        label: "Europe/Zurich (Suisse — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Luxembourg",
        label: "Europe/Luxembourg (été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/London",
        label: "Europe/London (Royaume-Uni — GMT/BST auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Berlin",
        label: "Europe/Berlin (Allemagne — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Madrid",
        label: "Europe/Madrid (Espagne — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Rome",
        label: "Europe/Rome (Italie — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "Europe/Lisbon",
        label: "Europe/Lisbon (Portugal — été/hiver auto)",
        group: TimezoneGroup::Europe,
    },
    TimezonePreset {
        id: "America/Montreal",
        label: "America/Montreal (Québec — été/hiver auto)",
        group: TimezoneGroup::Americas,
    },
    TimezonePreset {
        id: "America/Toronto",
        label: "America/Toronto (Canada Est — été/hiver auto)",
        group: TimezoneGroup::Americas,
    },
    TimezonePreset {
        id: "America/New_York",
        label: "America/New_York (États-Unis Est — été/hiver auto)",
        group: TimezoneGroup::Americas,
    },
    TimezonePreset {
        id: "America/Chicago",
        label: "America/Chicago (États-Unis Centre — été/hiver auto)",
        group: TimezoneGroup::Americas,
    },
    TimezonePreset {
        id: "America/Los_Angeles",
        label: "America/Los_Angeles (États-Unis Ouest — été/hiver auto)",
        group: TimezoneGroup::Americas,
    },
    TimezonePreset {
        id: "America/Guadeloupe",
        label: "America/Guadeloupe (UTC-04:00)",
        group: TimezoneGroup::Overseas,
    },
    TimezonePreset {
        id: "America/Martinique",
        label: "America/Martinique (UTC-04:00)",
        group: TimezoneGroup::Overseas,
    },
    TimezonePreset {
        id: "America/Cayenne",
        label: "America/Cayenne (UTC-03:00)",
        group: TimezoneGroup::Overseas,
    },
    TimezonePreset {
        id: "Indian/Reunion",
        label: "Indian/Reunion (La Réunion — UTC+04:00)",
        group: TimezoneGroup::Overseas,
    },
    TimezonePreset {
        id: "Pacific/Noumea",
        label: "Pacific/Noumea (Nouvelle-Calédonie — UTC+11:00)",
        group: TimezoneGroup::Overseas,
    },
    TimezonePreset {
        id: "Pacific/Tahiti",
        label: "Pacific/Tahiti (Polynésie — UTC-10:00)",
        group: TimezoneGroup::Overseas,
    },
];

/// État courant de l'horloge dans un fuseau horaire donné (`GET /api/v1/system/timezone`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockInfo {
    pub timezone: String,
    pub utc_offset: String,
    pub dst_state: DstState,
    pub formatted_local: String,
    pub formatted_utc: String,
}

/// Corps de `PUT /api/v1/system/timezone`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTimezoneRequest {
    pub timezone: String,
}

/// Analyse un identifiant de fuseau horaire IANA ou un alias courant (`UTC`, `GMT`, `Z`).
pub fn parse_timezone(raw: &str) -> Option<Tz> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "UTC" | "Z" | "ETC/UTC" => Some(Tz::UTC),
        "GMT" | "ETC/GMT" => Some(Tz::Etc__GMT),
        _ => trimmed.parse::<Tz>().ok(),
    }
}

/// Retourne le nom IANA canonique d'un fuseau horaire valide.
pub fn canonical_timezone(raw: &str) -> Option<String> {
    parse_timezone(raw).map(|tz| tz.name().to_owned())
}

/// Indique si le fuseau horaire observe un changement d'heure (été/hiver) au cours de l'année.
fn observes_dst(tz: Tz, year: i32) -> bool {
    let jan = Utc.with_ymd_and_hms(year, 1, 15, 12, 0, 0).single();
    let jul = Utc.with_ymd_and_hms(year, 7, 15, 12, 0, 0).single();
    match (jan, jul) {
        (Some(a), Some(b)) => {
            a.with_timezone(&tz).offset().dst_offset().num_seconds() != 0
                || b.with_timezone(&tz).offset().dst_offset().num_seconds() != 0
        }
        _ => false,
    }
}

/// Formate un décalage en secondes en `UTC+01:00` ou `UTC-04:00`.
pub fn format_utc_offset(total_seconds: i32) -> String {
    let sign = if total_seconds >= 0 { '+' } else { '-' };
    let abs = total_seconds.unsigned_abs() as i32;
    let hours = abs / SECONDS_PER_HOUR;
    let minutes = (abs % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    format!("UTC{sign}{hours:02}:{minutes:02}")
}

/// Décrit un instant dans le fuseau demandé (repli sur UTC si le fuseau est invalide).
pub fn describe_instant(now: DateTime<Utc>, tz_name: &str) -> ClockInfo {
    let tz = parse_timezone(tz_name).unwrap_or(Tz::UTC);
    let local = now.with_timezone(&tz);
    let offset = local.offset();
    let total_seconds = offset.fix().local_minus_utc();
    let dst_seconds = offset.dst_offset().num_seconds();
    let dst_state = if dst_seconds != 0 {
        DstState::Summer
    } else if observes_dst(tz, local.year()) {
        DstState::Winter
    } else {
        DstState::Universal
    };
    ClockInfo {
        timezone: tz.name().to_owned(),
        utc_offset: format_utc_offset(total_seconds),
        dst_state,
        formatted_local: local.format("%d/%m/%Y %H:%M:%S").to_string(),
        formatted_utc: now.format("%d/%m/%Y %H:%M:%S UTC").to_string(),
    }
}

/// Formate une date UTC dans le fuseau indiqué (`DD/MM/YYYY HH:MM` ou `DD/MM/YYYY HH:MM:SS`).
pub fn format_in_timezone(value: DateTime<Utc>, tz_name: &str, with_seconds: bool) -> String {
    let tz = parse_timezone(tz_name).unwrap_or(Tz::UTC);
    let local = value.with_timezone(&tz);
    let pattern = if with_seconds {
        "%d/%m/%Y %H:%M:%S"
    } else {
        "%d/%m/%Y %H:%M"
    };
    if tz == Tz::UTC || tz == Tz::Etc__GMT {
        format!(
            "{} {}",
            local.format(pattern),
            tz.name().trim_start_matches("Etc/")
        )
    } else {
        local.format(pattern).to_string()
    }
}

/// Jour de la semaine (1 = lundi … 7 = dimanche) et heure locale (0..=23) pour les plannings workers.
pub fn local_weekday_and_hour(now: DateTime<Utc>, tz_name: &str) -> (u8, u8) {
    let tz = parse_timezone(tz_name).unwrap_or(Tz::UTC);
    let local = now.with_timezone(&tz);
    (
        local.weekday().number_from_monday() as u8,
        local.hour() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_iana_and_common_aliases() {
        assert_eq!(
            canonical_timezone("Europe/Paris").as_deref(),
            Some("Europe/Paris")
        );
        assert_eq!(canonical_timezone("utc").as_deref(), Some("UTC"));
        assert_eq!(canonical_timezone("GMT").as_deref(), Some("Etc/GMT"));
        assert_eq!(
            canonical_timezone("Etc/GMT-1").as_deref(),
            Some("Etc/GMT-1")
        );
        assert_eq!(canonical_timezone("Mars/Olympus"), None);
        assert_eq!(canonical_timezone("   "), None);
    }

    #[test]
    fn detects_winter_and_summer_time_in_paris() {
        let winter = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).single();
        let summer = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).single();
        let (Some(winter), Some(summer)) = (winter, summer) else {
            panic!("dates fixes valides");
        };

        let w = describe_instant(winter, "Europe/Paris");
        assert_eq!(w.utc_offset, "UTC+01:00");
        assert_eq!(w.dst_state, DstState::Winter);
        assert_eq!(w.formatted_local, "15/01/2026 13:00:00");

        let s = describe_instant(summer, "Europe/Paris");
        assert_eq!(s.utc_offset, "UTC+02:00");
        assert_eq!(s.dst_state, DstState::Summer);
        assert_eq!(s.formatted_local, "15/07/2026 14:00:00");
    }

    #[test]
    fn universal_and_fixed_offsets_never_switch_dst() {
        let summer = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).single();
        let Some(summer) = summer else {
            panic!("date fixe valide");
        };

        let utc = describe_instant(summer, "UTC");
        assert_eq!(utc.utc_offset, "UTC+00:00");
        assert_eq!(utc.dst_state, DstState::Universal);

        let fixed = describe_instant(summer, "Etc/GMT-1");
        assert_eq!(fixed.utc_offset, "UTC+01:00");
        assert_eq!(fixed.dst_state, DstState::Universal);
        assert_eq!(fixed.formatted_local, "15/07/2026 13:00:00");
    }

    #[test]
    fn weekday_and_hour_respect_midnight_rollover() {
        // Dimanche 23h30 UTC = Lundi 00h30 à Paris en heure d'hiver.
        let sunday_night = Utc.with_ymd_and_hms(2026, 1, 18, 23, 30, 0).single();
        let Some(sunday_night) = sunday_night else {
            panic!("date fixe valide");
        };
        assert_eq!(local_weekday_and_hour(sunday_night, "UTC"), (7, 23));
        assert_eq!(local_weekday_and_hour(sunday_night, "Europe/Paris"), (1, 0));
    }

    #[test]
    fn all_presets_are_valid_iana_identifiers() {
        for preset in TIMEZONE_PRESETS {
            assert!(
                parse_timezone(preset.id).is_some(),
                "preset invalide : {}",
                preset.id
            );
        }
    }
}
