//! Mise en forme partagée entre le script historique et le client WASM.

use chrono::{Datelike, NaiveDate};

pub const BYTE_UNITS: [&str; 5] = ["o", "Kio", "Mio", "Gio", "Tio"];

/// Abréviations françaises des mois, de janvier à décembre.
const FRENCH_MONTHS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];

/// Date courte en français, indépendante du fuseau (ex. « 19 nov. 2026 »).
pub fn format_day(date: NaiveDate) -> String {
    format!(
        "{} {} {}",
        date.day(),
        FRENCH_MONTHS[date.month0() as usize],
        date.year()
    )
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return format!("0 {}", BYTE_UNITS[0]);
    }
    let mut index = 0;
    let mut scaled = bytes;
    while scaled >= 1024 && index < BYTE_UNITS.len() - 1 {
        scaled /= 1024;
        index += 1;
    }
    let divisor = 1024_u64.pow(index as u32) as f64;
    let rounded = ((bytes as f64 / divisor) * 100.0).round() / 100.0;
    format!("{} {}", trim_fixed(rounded), BYTE_UNITS[index])
}

fn trim_fixed(value: f64) -> String {
    let fixed = format!("{value:.2}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_day_uses_french_month_abbreviations() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 19);
        assert_eq!(date.map(format_day).as_deref(), Some("19 nov. 2026"));
        let first = NaiveDate::from_ymd_opt(2027, 1, 1);
        assert_eq!(first.map(format_day).as_deref(), Some("1 janv. 2027"));
    }

    #[test]
    fn format_bytes_scales_units() {
        assert_eq!(format_bytes(0), "0 o");
        assert_eq!(format_bytes(1536), "1.5 Kio");
    }
}
