//! Administration des comptes en ligne de commande : mot de passe oublié (ADR 0011).
//!
//! Il n'y a ni courriel ni question secrète : la personne qui accède au conteneur peut réinitialiser
//! un compte. Le nouveau mot de passe est généré, affiché une seule fois, et toutes les sessions du
//! compte sont fermées.

use crate::{
    error::{AppError, AppResult},
    password, repository,
};
use chacha20poly1305::aead::{OsRng, rand_core::RngCore};
use sqlx::SqlitePool;

const GENERATED_PASSWORD_BYTES: usize = 12;

fn generate_password() -> String {
    let mut bytes = [0u8; GENERATED_PASSWORD_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Remplace le mot de passe du compte `username` par un mot de passe généré, renvoyé une seule fois.
pub async fn reset_password(pool: &SqlitePool, username: &str) -> AppResult<String> {
    let (account, _) = repository::find_account_by_username(pool, username.trim())
        .await?
        .ok_or(AppError::NotFound)?;
    let new_password = generate_password();
    password::validate_password(&new_password)?;
    let hash = password::hash(new_password.clone()).await?;
    repository::update_password(pool, &account.id, &hash, None).await?;
    Ok(new_password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generated_passwords_are_long_and_distinct() {
        let first = generate_password();
        assert_eq!(first.len(), GENERATED_PASSWORD_BYTES * 2);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, generate_password());
    }
}
