//! Encodage et décodage Base64 standard (RFC 4648) sans dépendance externe.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode un tampon binaire en chaîne Base64 standard avec padding `=`.
pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn decode_val(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a' + 26)),
        b'0'..=b'9' => Some(u32::from(byte - b'0' + 52)),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Décode une chaîne Base64 standard (tolère les retours à la ligne) en octets bruts.
pub fn decode(input: &str) -> Result<Vec<u8>, &'static str> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !clean.len().is_multiple_of(4) {
        return Err("longueur base64 invalide");
    }
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    for (idx, quad) in clean.chunks_exact(4).enumerate() {
        let is_last = (idx + 1) * 4 == clean.len();
        let a = decode_val(quad[0]).ok_or("caractère base64 invalide")?;
        let b = decode_val(quad[1]).ok_or("caractère base64 invalide")?;
        let pad2 = quad[2] == b'=';
        let pad3 = quad[3] == b'=';
        if (pad2 && !pad3) || ((pad2 || pad3) && !is_last) {
            return Err("padding base64 invalide");
        }
        let c = if pad2 {
            0
        } else {
            decode_val(quad[2]).ok_or("caractère base64 invalide")?
        };
        let d = if pad3 {
            0
        } else {
            decode_val(quad[3]).ok_or("caractère base64 invalide")?
        };
        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        out.push(((triple >> 16) & 0xFF) as u8);
        if !pad2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if !pad3 {
            out.push((triple & 0xFF) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrips_arbitrary_bytes_and_rejects_corrupt_input() {
        for sample in [
            b"".as_slice(),
            b"f",
            b"fo",
            b"foo",
            b"Flux de Fichiers \x00\xff",
        ] {
            let encoded = encode(sample);
            assert_eq!(decode(&encoded), Ok(sample.to_vec()));
        }
        assert!(decode("abc").is_err());
        assert!(decode("ab=c").is_err());
        assert!(decode("ab==").is_ok());
    }
}
