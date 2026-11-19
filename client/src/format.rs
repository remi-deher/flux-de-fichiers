//! Formatage unifié des dates et horodatages dans le fuseau horaire actif de l'application.

use chrono::{DateTime, Utc};
use forgearr_shared::timezone::format_in_timezone;

const FALLBACK_TIMESTAMP_CHARS: usize = 19;

/// Date et heure formatées dans le fuseau horaire actif (synchronisé avec le central).
pub fn datetime(value: DateTime<Utc>) -> String {
    let tz = crate::timezone::current_timezone();
    format_in_timezone(value, &tz, false)
}

/// Horodatage RFC 3339 (ex. journaux applicatifs) formaté avec secondes dans le fuseau actif.
pub fn timestamp_rfc3339(raw: &str) -> String {
    let tz = crate::timezone::current_timezone();
    timestamp_in_timezone(raw, &tz)
}

/// Variante pure pour les tests : convertit un horodatage RFC 3339 dans un fuseau donné.
pub fn timestamp_in_timezone(raw: &str, tz: &str) -> String {
    match DateTime::parse_from_rfc3339(raw) {
        Ok(parsed) => format_in_timezone(parsed.to_utc(), tz, true),
        Err(_) => raw
            .chars()
            .take(FALLBACK_TIMESTAMP_CHARS)
            .map(|c| if c == 'T' { ' ' } else { c })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_format_names_the_time_zone() {
        assert_eq!(
            datetime(DateTime::<Utc>::UNIX_EPOCH),
            "01/01/1970 00:00 UTC"
        );
    }

    #[test]
    fn rfc3339_timestamp_converts_to_target_timezone_or_falls_back() {
        assert_eq!(
            timestamp_in_timezone("2026-01-15T17:10:06.123456Z", "Europe/Paris"),
            "15/01/2026 18:10:06"
        );
        assert_eq!(
            timestamp_in_timezone("2026-07-15T17:10:06.123456Z", "Europe/Paris"),
            "15/07/2026 19:10:06"
        );
        assert_eq!(
            timestamp_in_timezone("2026-11-18T17:10:06.123456Z", "UTC"),
            "18/11/2026 17:10:06 UTC"
        );
        assert_eq!(
            timestamp_in_timezone("pas-une-date", "Europe/Paris"),
            "pas-une-date"
        );
    }
}
