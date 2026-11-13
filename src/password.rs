//! Mots de passe : validation et hachage Argon2id (salage par compte, format PHC).

use crate::error::{AppError, AppResult};
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use std::sync::LazyLock;

/// Coût Argon2id réglé pour un petit NAS (OWASP : 19 Mio, 2 itérations, 1 voie).
const ARGON2_MEMORY_KIB: u32 = 19_456;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;

pub const USERNAME_MIN_LEN: usize = 3;
pub const USERNAME_MAX_LEN: usize = 40;
pub const PASSWORD_MIN_LEN: usize = 8;
/// Borne haute en octets : un mot de passe démesuré ne doit pas servir à saturer le serveur.
pub const PASSWORD_MAX_BYTES: usize = 256;

fn hasher() -> AppResult<Argon2<'static>> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .map_err(|_| AppError::Internal)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Identifiant normalisé (espaces retirés) : lettres, chiffres, `.`, `-` et `_`.
pub fn validate_username(raw: &str) -> AppResult<String> {
    let username = raw.trim();
    let valid_chars = username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if (USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&username.len()) && valid_chars {
        Ok(username.to_owned())
    } else {
        Err(AppError::Invalid)
    }
}

pub fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() >= PASSWORD_MIN_LEN && password.len() <= PASSWORD_MAX_BYTES {
        Ok(())
    } else {
        Err(AppError::Invalid)
    }
}

/// Haché PHC d'un mot de passe ; calculé hors du runtime asynchrone (Argon2 est volontairement coûteux).
pub async fn hash(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        hasher()?
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AppError::Internal)
    })
    .await
    .map_err(|_| AppError::Internal)?
}

/// Le mot de passe correspond-il au haché ? Un haché illisible ne correspond jamais.
pub async fn verify(password: String, stored_hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&stored_hash).is_ok_and(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false)
}

/// Haché d'une valeur aléatoire jetée : aucun mot de passe ne lui correspond, mais un identifiant
/// inconnu coûte le même temps de calcul qu'un identifiant connu.
static DECOY_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    let discarded = uuid::Uuid::new_v4().to_string();
    hasher()
        .and_then(|argon| {
            argon
                .hash_password(discarded.as_bytes(), &salt)
                .map(|hash| hash.to_string())
                .map_err(|_| AppError::Internal)
        })
        .unwrap_or_default()
});

pub async fn verify_decoy(password: String) -> bool {
    verify(password, DECOY_HASH.clone()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hash_is_argon2id_salted_and_verifiable() -> AppResult<()> {
        let first = hash("un mot de passe".into()).await?;
        let second = hash("un mot de passe".into()).await?;
        assert!(first.starts_with("$argon2id$"));
        assert_ne!(first, second);
        assert!(verify("un mot de passe".into(), first.clone()).await);
        assert!(!verify("un autre".into(), first).await);
        Ok(())
    }

    #[tokio::test]
    async fn test_unreadable_hash_never_matches_and_decoy_never_matches() {
        assert!(!verify("x".into(), "pas un hash".into()).await);
        assert!(!verify_decoy("decoy".into()).await);
    }

    #[test]
    fn test_username_rules() -> AppResult<()> {
        assert_eq!(validate_username("  admin ")?, "admin");
        assert_eq!(validate_username("jean.dupont-2")?, "jean.dupont-2");
        for invalid in [
            "",
            "ab",
            "a b c",
            "é",
            "x/y",
            &"x".repeat(USERNAME_MAX_LEN + 1),
        ] {
            assert!(validate_username(invalid).is_err(), "{invalid}");
        }
        Ok(())
    }

    #[test]
    fn test_password_rules() {
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("court").is_err());
        assert!(validate_password(&"x".repeat(PASSWORD_MAX_BYTES + 1)).is_err());
    }
}
