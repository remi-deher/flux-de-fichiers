//! Secrets opaques aléatoires et empreintes SHA-256 : l'unique endroit où l'on tire un secret, l'encode
//! en hexadécimal ou en calcule l'empreinte stockée (clés d'API, sessions, billets, jetons de remise).

use chacha20poly1305::aead::{OsRng, rand_core::RngCore};
use sha2::{Digest, Sha256};

/// Taille d'un secret opaque courant (billets, jetons de réception) : 256 bits, soit 64 caractères.
pub const OPAQUE_TOKEN_BYTES: usize = 32;

/// Longueur d'une empreinte SHA-256 en caractères hexadécimaux.
pub const SHA256_HEX_LEN: usize = 64;

/// Vérifie qu'une empreinte SHA-256 hexadécimale est syntaxiquement valide.
pub fn is_valid_sha256_hex(raw: &str) -> bool {
    raw.len() == SHA256_HEX_LEN && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Encode des octets en hexadécimal minuscule.
pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Empreinte SHA-256 en hexadécimal minuscule (64 caractères) : la seule forme stockée d'un secret.
pub fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    encode_hex(&Sha256::digest(data))
}

/// `bytes` octets tirés de l'aléa du système, en hexadécimal.
pub fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buffer);
    encode_hex(&buffer)
}

/// Nouveau secret opaque de [`OPAQUE_TOKEN_BYTES`] octets.
pub fn new_opaque_token() -> String {
    random_hex(OPAQUE_TOKEN_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(encode_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
        assert_eq!(encode_hex(&[]), "");
    }

    #[test]
    fn sha256_matches_the_known_vector_and_hides_the_secret() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_hex("abc"), sha256_hex(b"abc"));
        assert_eq!(sha256_hex(String::from("abc")), sha256_hex("abc"));
        assert_ne!(sha256_hex("abc"), sha256_hex("abd"));
        assert_eq!(sha256_hex("abc").len(), 64);
    }

    #[test]
    fn sha256_hex_validation_accepts_only_64_hex_digits() {
        assert!(is_valid_sha256_hex(&sha256_hex("abc")));
        assert!(is_valid_sha256_hex(&"AB".repeat(SHA256_HEX_LEN / 2)));
        assert!(!is_valid_sha256_hex(&"a".repeat(SHA256_HEX_LEN - 1)));
        assert!(!is_valid_sha256_hex(&"a".repeat(SHA256_HEX_LEN + 1)));
        assert!(!is_valid_sha256_hex(&"z".repeat(SHA256_HEX_LEN)));
        assert!(!is_valid_sha256_hex(""));
    }

    #[test]
    fn random_secrets_have_the_requested_length_and_never_repeat() {
        assert_eq!(random_hex(24).len(), 48);
        assert_ne!(random_hex(24), random_hex(24));
        let token = new_opaque_token();
        assert_eq!(token.len(), OPAQUE_TOKEN_BYTES * 2);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(token, new_opaque_token());
    }
}
