//! Chiffrement au repos des secrets (clés d'API de plugins) avec une clé locale générée dans `/config`.

use crate::error::{AppError, AppResult};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng},
};
use std::{
    fs,
    io::{ErrorKind, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const KEY_FILE_NAME: &str = "secret_key";
const KEY_FILE_MODE: u32 = 0o600;
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const HEX_RADIX: u32 = 16;
/// Préfixe d'une valeur chiffrée : version du format, puis nonce et texte chiffré en hexadécimal.
pub const ENCRYPTED_PREFIX: &str = "enc:v1:";

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(HEX_RADIX)?;
            let low = char::from(pair[1]).to_digit(HEX_RADIX)?;
            u8::try_from(high * HEX_RADIX + low).ok()
        })
        .collect()
}

/// Valeur déjà chiffrée par ce module ?
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

pub struct SecretBox {
    cipher: ChaCha20Poly1305,
}

impl SecretBox {
    /// Ouvre la clé du dossier de configuration ; la génère (lecture/écriture propriétaire seule) si elle manque.
    pub fn open(config_dir: &Path) -> AppResult<Self> {
        fs::create_dir_all(config_dir)?;
        let path = config_dir.join(KEY_FILE_NAME);
        let key_hex = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let key = ChaCha20Poly1305::generate_key(&mut OsRng);
                let created = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(KEY_FILE_MODE)
                    .open(&path);
                match created {
                    Ok(mut file) => {
                        file.write_all(encode_hex(&key).as_bytes())?;
                        encode_hex(&key)
                    }
                    // Une autre requête vient de la créer : on relit la sienne.
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        fs::read_to_string(&path)?
                    }
                    Err(error) => return Err(AppError::Storage(error)),
                }
            }
            Err(error) => return Err(AppError::Storage(error)),
        };
        let key_bytes = decode_hex(key_hex.trim())
            .filter(|bytes| bytes.len() == KEY_BYTES)
            .ok_or(AppError::Internal)?;
        Ok(Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(&key_bytes)),
        })
    }

    pub fn encrypt(&self, plaintext: &str) -> AppResult<String> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let sealed = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| AppError::Internal)?;
        let mut payload = nonce.to_vec();
        payload.extend_from_slice(&sealed);
        Ok(format!("{ENCRYPTED_PREFIX}{}", encode_hex(&payload)))
    }

    pub fn decrypt(&self, stored: &str) -> AppResult<String> {
        let payload = stored
            .strip_prefix(ENCRYPTED_PREFIX)
            .and_then(decode_hex)
            .filter(|bytes| bytes.len() > NONCE_BYTES)
            .ok_or(AppError::Invalid)?;
        let (nonce, sealed) = payload.split_at(NONCE_BYTES);
        let plaintext = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), sealed)
            .map_err(|_| AppError::Invalid)?;
        String::from_utf8(plaintext).map_err(|_| AppError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_round_trip_and_distinct_ciphertexts() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let first = secrets.encrypt("clef-sonarr")?;
        let second = secrets.encrypt("clef-sonarr")?;
        assert!(is_encrypted(&first));
        assert!(!first.contains("clef-sonarr"));
        assert_ne!(first, second);
        assert_eq!(secrets.decrypt(&first)?, "clef-sonarr");
        assert_eq!(secrets.decrypt(&second)?, "clef-sonarr");
        Ok(())
    }

    #[test]
    fn test_key_file_is_private_and_reused() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let first = SecretBox::open(dir.path())?;
        let mode = fs::metadata(dir.path().join(KEY_FILE_NAME))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, KEY_FILE_MODE);
        let sealed = first.encrypt("valeur")?;
        let reopened = SecretBox::open(dir.path())?;
        assert_eq!(reopened.decrypt(&sealed)?, "valeur");
        Ok(())
    }

    #[test]
    fn test_tampered_or_foreign_values_are_refused() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open(dir.path())?;
        let sealed = secrets.encrypt("valeur")?;
        let mut tampered = sealed.clone();
        tampered.pop();
        tampered.push(if sealed.ends_with('0') { '1' } else { '0' });
        assert!(matches!(secrets.decrypt(&tampered), Err(AppError::Invalid)));
        assert!(matches!(
            secrets.decrypt("en clair"),
            Err(AppError::Invalid)
        ));

        let other_dir = tempfile::tempdir()?;
        let other = SecretBox::open(other_dir.path())?;
        assert!(matches!(other.decrypt(&sealed), Err(AppError::Invalid)));
        Ok(())
    }

    #[test]
    fn test_corrupted_key_file_is_an_internal_failure() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join(KEY_FILE_NAME), "pas de l'hexadécimal")?;
        assert!(matches!(
            SecretBox::open(dir.path()),
            Err(AppError::Internal)
        ));
        Ok(())
    }

    #[test]
    fn test_hex_helpers_round_trip() {
        let bytes = [0u8, 15, 16, 255];
        assert_eq!(decode_hex(&encode_hex(&bytes)), Some(bytes.to_vec()));
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
    }
}
