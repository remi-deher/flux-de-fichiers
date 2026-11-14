//! Dates affichées dans le fuseau et la langue du navigateur.

use chrono::{DateTime, Utc};

/// Date et heure locales, en français.
#[cfg(target_arch = "wasm32")]
pub fn datetime(value: DateTime<Utc>) -> String {
    let millis = value.timestamp_millis() as f64;
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(millis));
    String::from(date.to_locale_string("fr-FR", &wasm_bindgen::JsValue::UNDEFINED))
}

/// Hors navigateur (tests) : heure UTC explicite.
#[cfg(not(target_arch = "wasm32"))]
pub fn datetime(value: DateTime<Utc>) -> String {
    value.format("%d/%m/%Y %H:%M UTC").to_string()
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
}
