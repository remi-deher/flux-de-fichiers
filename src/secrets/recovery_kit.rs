//! Kit de récupération : le trousseau (clé active puis clés retirées) chiffré par une phrase de passe.
//!
//! Dérivation Argon2id (sel aléatoire, paramètres inscrits dans le kit), puis ChaCha20-Poly1305.
//! Le fichier exporté peut être rangé avec les sauvegardes : sans la phrase de passe il ne révèle rien.

use super::{KEY_BYTES, NONCE_BYTES, decode_hex, encode_hex, retired_keys};
use crate::error::{AppError, AppResult};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng, Payload, rand_core::RngCore},
};
use forgearr_shared::system::{
    RECOVERY_KIT_FORMAT, RECOVERY_PASSPHRASE_MIN_LEN, RecoveryKdf, RecoveryKdfAlgorithm,
    RecoveryKit,
};
use std::{fs, io::ErrorKind, path::Path};

const KIT_VERSION: u32 = 1;
/// Données associées : un kit ne se confond avec aucune autre valeur chiffrée.
const KIT_AAD: &[u8] = b"flux-recovery-kit:v1";
const SALT_BYTES: usize = 16;
/// Coût Argon2id à l'export (OWASP : 19 Mio, 2 itérations, 1 voie), comme les mots de passe.
const KDF_MEMORY_KIB: u32 = 19_456;
const KDF_ITERATIONS: u32 = 2;
const KDF_PARALLELISM: u32 = 1;
/// Bornes à l'import : un kit forgé ne doit pas faire travailler le central sans fin.
const KDF_MAX_MEMORY_KIB: u32 = 262_144;
const KDF_MAX_ITERATIONS: u32 = 10;
const KDF_MAX_PARALLELISM: u32 = 8;
const PASSPHRASE_MAX_BYTES: usize = 1024;
/// Témoin du dernier export : identifiant de la clé active à ce moment.
const EXPORTED_MARKER_FILE_NAME: &str = "recovery_kit.exported";

fn validate_passphrase(passphrase: &str) -> AppResult<()> {
    if passphrase.chars().count() >= RECOVERY_PASSPHRASE_MIN_LEN
        && passphrase.len() <= PASSPHRASE_MAX_BYTES
    {
        Ok(())
    } else {
        Err(AppError::Invalid)
    }
}

fn derive_cipher(passphrase: &str, kdf: &RecoveryKdf) -> AppResult<ChaCha20Poly1305> {
    let RecoveryKdfAlgorithm::Argon2id = kdf.algorithm;
    if kdf.memory_kib > KDF_MAX_MEMORY_KIB
        || kdf.iterations > KDF_MAX_ITERATIONS
        || kdf.parallelism > KDF_MAX_PARALLELISM
    {
        return Err(AppError::Invalid);
    }
    let salt = decode_hex(&kdf.salt).ok_or(AppError::Invalid)?;
    let params = Params::new(
        kdf.memory_kib,
        kdf.iterations,
        kdf.parallelism,
        Some(KEY_BYTES),
    )
    .map_err(|_| AppError::Invalid)?;
    let mut key = [0u8; KEY_BYTES];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|_| AppError::Invalid)?;
    Ok(ChaCha20Poly1305::new(Key::from_slice(&key)))
}

/// Trousseau courant en hexadécimal : clé active d'abord, puis clés retirées.
fn keyring_hex(config_dir: &Path, external_key_file: Option<&Path>) -> AppResult<Vec<String>> {
    let active_path = external_key_file
        .map(Path::to_path_buf)
        .unwrap_or_else(|| super::SecretBox::local_key_path(config_dir));
    let mut keys = vec![fs::read_to_string(active_path)?.trim().to_owned()];
    keys.extend(retired_keys::read_lines(config_dir)?);
    Ok(keys)
}

/// Chiffre le trousseau courant avec `passphrase`. Coûteux : à appeler hors du runtime asynchrone.
pub fn export(
    config_dir: &Path,
    external_key_file: Option<&Path>,
    passphrase: &str,
) -> AppResult<RecoveryKit> {
    validate_passphrase(passphrase)?;
    // Ouvrir le trousseau valide les fichiers et crée la clé locale si elle manque encore.
    let key_id = super::SecretBox::open_with(config_dir, external_key_file)?
        .key_id()
        .to_owned();
    let keys = keyring_hex(config_dir, external_key_file)?;
    let mut salt = [0u8; SALT_BYTES];
    OsRng.fill_bytes(&mut salt);
    let kdf = RecoveryKdf {
        algorithm: RecoveryKdfAlgorithm::Argon2id,
        memory_kib: KDF_MEMORY_KIB,
        iterations: KDF_ITERATIONS,
        parallelism: KDF_PARALLELISM,
        salt: encode_hex(&salt),
    };
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let plaintext = keys.join("\n");
    let ciphertext = derive_cipher(passphrase, &kdf)?
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext.as_bytes(),
                aad: KIT_AAD,
            },
        )
        .map_err(|_| AppError::Internal)?;
    Ok(RecoveryKit {
        format: RECOVERY_KIT_FORMAT.to_owned(),
        version: KIT_VERSION,
        key_id,
        created_at: chrono::Utc::now(),
        kdf,
        nonce: encode_hex(&nonce),
        ciphertext: encode_hex(&ciphertext),
    })
}

/// Clés du kit, en hexadécimal. Phrase de passe fausse ou kit altéré : `Invalid`.
pub fn open(kit: &RecoveryKit, passphrase: &str) -> AppResult<Vec<String>> {
    if kit.format != RECOVERY_KIT_FORMAT || kit.version != KIT_VERSION {
        return Err(AppError::Invalid);
    }
    let nonce = decode_hex(&kit.nonce)
        .filter(|bytes| bytes.len() == NONCE_BYTES)
        .ok_or(AppError::Invalid)?;
    let ciphertext = decode_hex(&kit.ciphertext).ok_or(AppError::Invalid)?;
    let plaintext = derive_cipher(passphrase, &kit.kdf)?
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: KIT_AAD,
            },
        )
        .map_err(|_| AppError::Invalid)?;
    let text = String::from_utf8(plaintext).map_err(|_| AppError::Invalid)?;
    let keys: Vec<String> = text.lines().map(str::to_owned).collect();
    for key in &keys {
        super::SecretBox::from_key_hex(key).map_err(|_| AppError::Invalid)?;
    }
    Ok(keys)
}

/// Note que le trousseau de la clé `key_id` a été exporté.
pub fn mark_exported(config_dir: &Path, key_id: &str) -> AppResult<()> {
    crate::worker_settings::write_private(&config_dir.join(EXPORTED_MARKER_FILE_NAME), key_id)
        .map_err(|error| {
            tracing::error!(error = %error, "écriture du témoin d'export du kit impossible");
            AppError::Internal
        })
}

/// Clé active lors du dernier export, si un kit a déjà été exporté.
pub fn exported_key_id(config_dir: &Path) -> AppResult<Option<String>> {
    match fs::read_to_string(config_dir.join(EXPORTED_MARKER_FILE_NAME)) {
        Ok(text) => Ok(Some(text.trim().to_owned())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{SecretBox, retire_key};

    const PASSPHRASE: &str = "une phrase assez longue";

    #[test]
    fn test_the_kit_round_trips_the_whole_keyring() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let old_hex = SecretBox::generate_key_hex();
        retire_key(dir.path(), &old_hex)?;
        let active = SecretBox::open_with(dir.path(), None)?;
        let kit = export(dir.path(), None, PASSPHRASE)?;
        assert_eq!(kit.key_id, active.key_id());
        assert!(!kit.ciphertext.contains(&old_hex));
        let keys = open(&kit, PASSPHRASE)?;
        assert_eq!(keys.len(), 2);
        assert_eq!(SecretBox::from_key_hex(&keys[0])?.key_id(), active.key_id());
        assert_eq!(keys[1], old_hex);
        Ok(())
    }

    #[test]
    fn test_a_wrong_passphrase_or_a_tampered_kit_is_refused() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let kit = export(dir.path(), None, PASSPHRASE)?;
        assert!(matches!(
            open(&kit, "une autre phrase longue"),
            Err(AppError::Invalid)
        ));
        let mut tampered = kit.clone();
        tampered.ciphertext.replace_range(..2, "00");
        assert!(matches!(
            open(&tampered, PASSPHRASE),
            Err(AppError::Invalid)
        ));
        let mut greedy = kit;
        greedy.kdf.memory_kib = KDF_MAX_MEMORY_KIB + 1;
        assert!(matches!(open(&greedy, PASSPHRASE), Err(AppError::Invalid)));
        Ok(())
    }

    #[test]
    fn test_a_short_passphrase_is_refused() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        SecretBox::open_with(dir.path(), None)?;
        assert!(matches!(
            export(dir.path(), None, "courte"),
            Err(AppError::Invalid)
        ));
        Ok(())
    }

    #[test]
    fn test_the_export_marker_remembers_the_key_id() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        assert_eq!(exported_key_id(dir.path())?, None);
        mark_exported(dir.path(), "abcd1234")?;
        assert_eq!(exported_key_id(dir.path())?.as_deref(), Some("abcd1234"));
        Ok(())
    }
}
