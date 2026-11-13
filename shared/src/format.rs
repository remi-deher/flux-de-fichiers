//! Mise en forme partagée entre le script historique et le client WASM.

pub const BYTE_UNITS: [&str; 5] = ["o", "Kio", "Mio", "Gio", "Tio"];

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
