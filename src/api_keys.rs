//! Clés d'API des machines : `fdf_<id>_<secret>`, seul le haché SHA-256 du secret est conservé.

use crate::{
    error::{AppError, AppResult},
    repository::{self, ApiKeyRecord},
};
use chacha20poly1305::aead::{OsRng, rand_core::RngCore};
use flux_shared::Role;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use subtle::ConstantTimeEq;

pub const KEY_PREFIX: &str = "fdf_";
const KEY_ID_BYTES: usize = 4;
const KEY_SECRET_BYTES: usize = 24;
const KEY_NAME_MAX_LEN: usize = 80;

/// Clé tout juste créée : la valeur complète n'existe qu'ici et n'est montrée qu'une fois.
#[derive(Debug)]
pub struct IssuedKey {
    pub record: ApiKeyRecord,
    pub raw: String,
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

/// Sépare une clé présentée en identifiant et secret ; `None` si le format est inconnu.
pub fn parse(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix(KEY_PREFIX)?;
    let (id, secret) = rest.split_once('_')?;
    let hex = |text: &str, expected: usize| {
        text.len() == expected * 2 && text.chars().all(|c| c.is_ascii_hexdigit())
    };
    (hex(id, KEY_ID_BYTES) && hex(secret, KEY_SECRET_BYTES)).then_some((id, secret))
}

fn validate_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > KEY_NAME_MAX_LEN
        || name.chars().any(char::is_control)
    {
        return Err(AppError::Invalid);
    }
    Ok(name.to_owned())
}

/// Crée une clé nommée ; elle n'est jamais relisible ensuite.
pub async fn issue(pool: &SqlitePool, name: &str, role: Role) -> AppResult<IssuedKey> {
    let name = validate_name(name)?;
    let id = random_hex(KEY_ID_BYTES);
    let secret = random_hex(KEY_SECRET_BYTES);
    let record = repository::insert_api_key(pool, &id, &name, &hash_secret(&secret), role).await?;
    Ok(IssuedKey {
        record,
        raw: format!("{KEY_PREFIX}{id}_{secret}"),
    })
}

/// Installe une clé dont la valeur est imposée (tests, banc d'essai) ; le format doit être valide.
pub async fn install_known_key(
    pool: &SqlitePool,
    raw: &str,
    name: &str,
    role: Role,
) -> AppResult<ApiKeyRecord> {
    let (id, secret) = parse(raw).ok_or(AppError::Invalid)?;
    repository::insert_api_key(pool, id, &validate_name(name)?, &hash_secret(secret), role).await
}

/// Clé non révoquée correspondant à la valeur présentée ; la dernière utilisation est mise à jour.
pub async fn authenticate(pool: &SqlitePool, raw: &str) -> AppResult<Option<ApiKeyRecord>> {
    let Some((id, secret)) = parse(raw) else {
        return Ok(None);
    };
    let Some((record, stored_hash)) = repository::find_active_api_key(pool, id).await? else {
        return Ok(None);
    };
    let matches = hash_secret(secret).as_bytes().ct_eq(stored_hash.as_bytes());
    if !bool::from(matches) {
        return Ok(None);
    }
    repository::touch_api_key(pool, id).await?;
    Ok(Some(record))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_accepts_only_the_exact_format() {
        let id = "ab12cd34";
        let secret = "0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(parse(&format!("fdf_{id}_{secret}")), Some((id, secret)));
        for bad in [
            "",
            "fdf_",
            "fdf_ab12cd34",
            &format!("xxx_{id}_{secret}"),
            &format!("fdf_{id}_{}", &secret[..10]),
            &format!("fdf_zz12cd34_{secret}"),
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn test_hash_secret_is_stable_and_hides_the_secret() {
        assert_eq!(hash_secret("abc"), hash_secret("abc"));
        assert_ne!(hash_secret("abc"), hash_secret("abd"));
        assert_eq!(hash_secret("abc").len(), 64);
    }

    #[test]
    fn test_name_validation() -> AppResult<()> {
        assert_eq!(validate_name("  Worker NAS ")?, "Worker NAS");
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(KEY_NAME_MAX_LEN + 1)).is_err());
        assert!(validate_name("tab\there").is_err());
        Ok(())
    }
}
