//! Chiffrement au repos des secrets (clés d'API de plugins) avec une clé locale générée dans `/config`.
//!
//! Trousseau : une clé active chiffre ; les clés retirées (`retired_keys`) ne servent plus qu'à
//! déchiffrer les valeurs qui portent encore leur identifiant.

pub mod recovery_kit;
mod retired_keys;
pub use retired_keys::{clear_retired_keys, retire_key, retired_keys_path};

use crate::error::{AppError, AppResult};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng, Payload},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{ErrorKind, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

const KEY_FILE_NAME: &str = "secret_key";
/// Variable d'environnement qui désigne un fichier de clé fourni par l'orchestrateur.
pub const KEY_FILE_ENV: &str = "SECRET_KEY_FILE";
/// Longueur de l'identifiant de clé, en caractères hexadécimaux.
const KEY_ID_HEX_LEN: usize = 8;
const KEY_FILE_MODE: u32 = 0o600;
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const HEX_RADIX: u32 = 16;
/// Préfixe commun des valeurs chiffrées : version du format, identifiant de clé, puis nonce et texte
/// chiffré en hexadécimal (`enc:v2:<id de clé>:<hex>`).
pub const ENCRYPTED_PREFIX: &str = "enc:";
const FORMAT_VERSION: &str = "v2";

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

/// Fichier de clé fourni par l'orchestrateur (`SECRET_KEY_FILE`), s'il est défini.
pub fn external_key_file() -> Option<std::path::PathBuf> {
    std::env::var(KEY_FILE_ENV)
        .ok()
        .map(std::path::PathBuf::from)
}

/// Valeur déjà chiffrée par ce module ?
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

/// Une clé du trousseau et son identifiant.
struct KeyEntry {
    cipher: ChaCha20Poly1305,
    key_id: String,
}

impl KeyEntry {
    fn from_bytes(key_bytes: &[u8]) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(Key::from_slice(key_bytes)),
            key_id: key_id_of(key_bytes),
        }
    }

    fn from_hex(key_hex: &str) -> AppResult<Self> {
        let key_bytes = decode_hex(key_hex.trim())
            .filter(|bytes| bytes.len() == KEY_BYTES)
            .ok_or(AppError::Internal)?;
        Ok(Self::from_bytes(&key_bytes))
    }
}

pub struct SecretBox {
    active: KeyEntry,
    retired: Vec<KeyEntry>,
}

fn key_id_of(key_bytes: &[u8]) -> String {
    let digest: String = Sha256::digest(key_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    digest[..KEY_ID_HEX_LEN].to_owned()
}

impl SecretBox {
    /// Ouvre la clé : le fichier de `SECRET_KEY_FILE` s'il est défini, sinon `secret_key` du dossier
    /// de configuration, généré (lecture et écriture propriétaire seules) s'il manque.
    pub fn open(config_dir: &Path) -> AppResult<Self> {
        Self::open_with(config_dir, external_key_file().as_deref())
    }

    /// Clé active (fichier externe ou locale) et clés retirées du dossier de configuration.
    pub fn open_with(config_dir: &Path, external_key_file: Option<&Path>) -> AppResult<Self> {
        let key_hex = match external_key_file {
            Some(path) => fs::read_to_string(path)?,
            None => Self::read_or_create_local_key(config_dir)?,
        };
        let active = KeyEntry::from_hex(&key_hex)?;
        let retired = retired_keys::load(config_dir)?
            .into_iter()
            .filter(|entry| entry.key_id != active.key_id)
            .collect();
        Ok(Self { active, retired })
    }

    /// Boîte construite sur une seule clé de 32 octets écrite en hexadécimal, sans clé retirée.
    pub fn from_key_hex(key_hex: &str) -> AppResult<Self> {
        Ok(Self {
            active: KeyEntry::from_hex(key_hex)?,
            retired: Vec::new(),
        })
    }

    /// Nouvelle clé aléatoire, en hexadécimal, prête à être écrite dans un fichier de clé.
    pub fn generate_key_hex() -> String {
        encode_hex(&ChaCha20Poly1305::generate_key(&mut OsRng))
    }

    /// Boîte dérivée du secret d'un worker pour protéger les secrets d'un bail `(job_id, lease_epoch)`.
    pub fn for_worker_lease(
        worker_bearer_key: &str,
        job_id: uuid::Uuid,
        lease_epoch: i64,
    ) -> AppResult<Self> {
        let trimmed = worker_bearer_key.trim();
        if trimmed.is_empty() {
            return Err(AppError::Unauthorized);
        }
        let mut hasher = Sha256::new();
        hasher.update(b"flux-lease-secrets-v1:");
        hasher.update(trimmed.as_bytes());
        hasher.update(b":");
        hasher.update(job_id.as_bytes());
        hasher.update(b":");
        hasher.update(lease_epoch.to_be_bytes());
        Ok(Self {
            active: KeyEntry::from_bytes(&hasher.finalize()),
            retired: Vec::new(),
        })
    }

    /// Fichier de la clé locale dans le dossier de configuration.
    pub fn local_key_path(config_dir: &Path) -> std::path::PathBuf {
        config_dir.join(KEY_FILE_NAME)
    }

    fn read_or_create_local_key(config_dir: &Path) -> AppResult<String> {
        fs::create_dir_all(config_dir)?;
        let path = config_dir.join(KEY_FILE_NAME);
        match fs::read_to_string(&path) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == ErrorKind::NotFound => Self::create_local_key(&path),
            Err(error) => Err(AppError::Storage(error)),
        }
    }

    /// Crée la clé locale sans qu'un lecteur concurrent puisse en voir une moitié : elle est écrite en
    /// entier dans un fichier temporaire, puis publiée d'un seul geste. Si une autre requête l'a publiée
    /// entre-temps, on relit la sienne.
    fn create_local_key(path: &Path) -> AppResult<String> {
        let key_hex = encode_hex(&ChaCha20Poly1305::generate_key(&mut OsRng));
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(KEY_FILE_MODE)
            .open(&temporary)?;
        file.write_all(key_hex.as_bytes())?;
        file.sync_all()?;
        drop(file);
        let published = fs::hard_link(&temporary, path);
        let _ = fs::remove_file(&temporary);
        match published {
            Ok(()) => Ok(key_hex),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(fs::read_to_string(path)?),
            Err(error) => Err(AppError::Storage(error)),
        }
    }

    /// Identifiant de la clé active, inscrit dans chaque valeur chiffrée.
    pub fn key_id(&self) -> &str {
        &self.active.key_id
    }

    /// Identifiants des clés retirées, encore utilisées pour déchiffrer.
    pub fn retired_key_ids(&self) -> Vec<&str> {
        self.retired
            .iter()
            .map(|entry| entry.key_id.as_str())
            .collect()
    }

    fn entry_for(&self, key_id: &str) -> Option<&KeyEntry> {
        std::iter::once(&self.active)
            .chain(&self.retired)
            .find(|entry| entry.key_id == key_id)
    }

    /// Chiffre `plaintext` en le liant à `aad` (par exemple `plugin:sonarr:apiKey`) : la valeur
    /// ne se déchiffre pas si on la copie vers un autre champ.
    pub fn encrypt(&self, plaintext: &str, aad: &str) -> AppResult<String> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let sealed = self
            .active
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| AppError::Internal)?;
        let mut payload = nonce.to_vec();
        payload.extend_from_slice(&sealed);
        Ok(format!(
            "{ENCRYPTED_PREFIX}{FORMAT_VERSION}:{}:{}",
            self.active.key_id,
            encode_hex(&payload)
        ))
    }

    /// Déchiffre avec la clé du trousseau dont l'identifiant figure dans la valeur.
    pub fn decrypt(&self, stored: &str, aad: &str) -> AppResult<String> {
        let mut parts = stored
            .strip_prefix(ENCRYPTED_PREFIX)
            .ok_or(AppError::Invalid)?
            .splitn(3, ':');
        let (version, key_id, hex) = (parts.next(), parts.next(), parts.next());
        if version != Some(FORMAT_VERSION) {
            return Err(AppError::Invalid);
        }
        let entry = key_id
            .and_then(|id| self.entry_for(id))
            .ok_or(AppError::Invalid)?;
        let payload = hex
            .and_then(decode_hex)
            .filter(|bytes| bytes.len() > NONCE_BYTES)
            .ok_or(AppError::Invalid)?;
        let (nonce, sealed) = payload.split_at(NONCE_BYTES);
        let plaintext = entry
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| AppError::Invalid)?;
        String::from_utf8(plaintext).map_err(|_| AppError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const AAD: &str = "plugin:plugin.demo:apiKey";

    #[test]
    fn test_round_trip_and_distinct_ciphertexts() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open_with(dir.path(), None)?;
        let first = secrets.encrypt("clef-sonarr", AAD)?;
        let second = secrets.encrypt("clef-sonarr", AAD)?;
        assert!(is_encrypted(&first));
        assert!(first.starts_with(&format!("enc:v2:{}:", secrets.key_id())));
        assert!(!first.contains("clef-sonarr"));
        assert_ne!(first, second);
        assert_eq!(secrets.decrypt(&first, AAD)?, "clef-sonarr");
        assert_eq!(secrets.decrypt(&second, AAD)?, "clef-sonarr");
        Ok(())
    }

    #[test]
    fn test_a_value_cannot_be_moved_to_another_field() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open_with(dir.path(), None)?;
        let sealed = secrets.encrypt("valeur", AAD)?;
        assert!(matches!(
            secrets.decrypt(&sealed, "plugin:plugin.autre:apiKey"),
            Err(AppError::Invalid)
        ));
        assert!(matches!(
            secrets.decrypt(&sealed, ""),
            Err(AppError::Invalid)
        ));
        Ok(())
    }

    #[test]
    fn test_key_file_is_private_reused_and_gives_a_stable_key_id() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let first = SecretBox::open_with(dir.path(), None)?;
        let mode = fs::metadata(dir.path().join(KEY_FILE_NAME))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, KEY_FILE_MODE);
        let sealed = first.encrypt("valeur", AAD)?;
        let reopened = SecretBox::open_with(dir.path(), None)?;
        assert_eq!(reopened.key_id(), first.key_id());
        assert_eq!(reopened.key_id().len(), KEY_ID_HEX_LEN);
        assert_eq!(reopened.decrypt(&sealed, AAD)?, "valeur");
        Ok(())
    }

    #[test]
    fn test_an_external_key_file_replaces_the_local_one() -> AppResult<()> {
        let config = tempfile::tempdir()?;
        let external_dir = tempfile::tempdir()?;
        let external = external_dir.path().join("cle-orchestrateur");
        fs::write(&external, "ab".repeat(KEY_BYTES))?;

        let secrets = SecretBox::open_with(config.path(), Some(&external))?;
        assert!(!config.path().join(KEY_FILE_NAME).exists());
        let sealed = secrets.encrypt("valeur", AAD)?;
        let again = SecretBox::open_with(config.path(), Some(&external))?;
        assert_eq!(again.decrypt(&sealed, AAD)?, "valeur");

        fs::write(&external, "pas de l'hexadécimal")?;
        assert!(matches!(
            SecretBox::open_with(config.path(), Some(&external)),
            Err(AppError::Internal)
        ));
        Ok(())
    }

    #[test]
    fn test_tampered_foreign_or_other_key_values_are_refused() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let secrets = SecretBox::open_with(dir.path(), None)?;
        let sealed = secrets.encrypt("valeur", AAD)?;
        let mut tampered = sealed.clone();
        tampered.pop();
        tampered.push(if sealed.ends_with('0') { '1' } else { '0' });
        assert!(matches!(
            secrets.decrypt(&tampered, AAD),
            Err(AppError::Invalid)
        ));
        assert!(matches!(
            secrets.decrypt("en clair", AAD),
            Err(AppError::Invalid)
        ));
        assert!(matches!(
            secrets.decrypt("enc:v2:abcdef", AAD),
            Err(AppError::Invalid)
        ));

        let other_dir = tempfile::tempdir()?;
        let other = SecretBox::open_with(other_dir.path(), None)?;
        assert_ne!(other.key_id(), secrets.key_id());
        assert!(matches!(
            other.decrypt(&sealed, AAD),
            Err(AppError::Invalid)
        ));
        Ok(())
    }

    #[test]
    fn test_corrupted_key_file_is_an_internal_failure() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join(KEY_FILE_NAME), "pas de l'hexadécimal")?;
        assert!(matches!(
            SecretBox::open_with(dir.path(), None),
            Err(AppError::Internal)
        ));
        Ok(())
    }

    #[test]
    fn test_concurrent_first_opens_agree_on_one_complete_key() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().to_path_buf();
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    SecretBox::open_with(&path, None).map(|b| b.key_id().to_owned())
                })
            })
            .collect();
        let ids: Vec<String> = handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| AppError::Internal)?)
            .collect::<AppResult<_>>()?;
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
        // Aucun fichier temporaire ne subsiste, et la clé publiée est complète.
        let names: Vec<String> = fs::read_dir(dir.path())?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [KEY_FILE_NAME.to_owned()]);
        assert_eq!(
            fs::read_to_string(dir.path().join(KEY_FILE_NAME))?.len(),
            KEY_BYTES * 2
        );
        Ok(())
    }

    /// Remplace la clé locale par une neuve après avoir retiré l'ancienne ; renvoie l'ancienne boîte.
    fn replace_local_key_keeping_old(dir: &Path) -> AppResult<SecretBox> {
        let old = SecretBox::open_with(dir, None)?;
        let old_hex = fs::read_to_string(dir.join(KEY_FILE_NAME))?;
        retire_key(dir, &old_hex)?;
        fs::write(dir.join(KEY_FILE_NAME), SecretBox::generate_key_hex())?;
        Ok(old)
    }

    #[test]
    fn test_retired_keys_still_decrypt_but_never_encrypt() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let old = replace_local_key_keeping_old(dir.path())?;
        let sealed_with_old = old.encrypt("ancienne", AAD)?;

        let ring = SecretBox::open_with(dir.path(), None)?;
        assert_ne!(ring.key_id(), old.key_id());
        assert_eq!(ring.retired_key_ids(), [old.key_id()]);
        assert_eq!(ring.decrypt(&sealed_with_old, AAD)?, "ancienne");
        assert!(
            ring.encrypt("neuve", AAD)?
                .starts_with(&format!("enc:v2:{}:", ring.key_id()))
        );
        assert!(matches!(
            ring.decrypt(&sealed_with_old, "plugin:plugin.autre:apiKey"),
            Err(AppError::Invalid)
        ));
        Ok(())
    }

    #[test]
    fn test_retire_key_is_private_deduplicated_and_validated() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        let key_hex = SecretBox::generate_key_hex();
        retire_key(dir.path(), &key_hex)?;
        retire_key(dir.path(), &format!("{key_hex}\n"))?;
        let path = retired_keys_path(dir.path());
        assert_eq!(fs::read_to_string(&path)?, key_hex);
        assert_eq!(
            fs::metadata(&path)?.permissions().mode() & 0o777,
            KEY_FILE_MODE
        );
        assert!(matches!(
            retire_key(dir.path(), "pas une clé"),
            Err(AppError::Internal)
        ));
        Ok(())
    }

    #[test]
    fn test_the_active_key_is_not_listed_as_retired() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        SecretBox::open_with(dir.path(), None)?;
        let active_hex = fs::read_to_string(dir.path().join(KEY_FILE_NAME))?;
        retire_key(dir.path(), &active_hex)?;
        assert!(
            SecretBox::open_with(dir.path(), None)?
                .retired_key_ids()
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn test_retired_keys_apply_with_an_external_key_file() -> AppResult<()> {
        let config = tempfile::tempdir()?;
        let old = replace_local_key_keeping_old(config.path())?;
        let sealed = old.encrypt("valeur", AAD)?;
        let external = config.path().join("cle-orchestrateur");
        fs::write(&external, SecretBox::generate_key_hex())?;
        let ring = SecretBox::open_with(config.path(), Some(&external))?;
        assert_eq!(ring.decrypt(&sealed, AAD)?, "valeur");
        Ok(())
    }

    #[test]
    fn test_a_corrupted_retired_key_file_is_an_internal_failure() -> AppResult<()> {
        let dir = tempfile::tempdir()?;
        fs::write(retired_keys_path(dir.path()), "pas de l'hexadécimal")?;
        assert!(matches!(
            SecretBox::open_with(dir.path(), None),
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
