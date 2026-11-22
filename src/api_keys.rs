//! Clés d'API des machines : `fdf_<id>_<secret>`, seul le haché SHA-256 du secret est conservé.

use crate::{
    error::{AppError, AppResult},
    opaque_token::{random_hex, sha256_hex},
    repository::{self, ApiKeyRecord},
};
use forgearr_shared::{Permission, Role};
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

fn validate_scopes(
    role: Role,
    scopes: Option<&[Permission]>,
) -> AppResult<Option<Vec<Permission>>> {
    let Some(raw) = scopes else {
        return Ok(None);
    };
    let mut deduped = Vec::with_capacity(raw.len());
    for &perm in raw {
        if !role.allows(perm) {
            return Err(AppError::Invalid);
        }
        if !deduped.contains(&perm) {
            deduped.push(perm);
        }
    }
    if deduped.is_empty() {
        return Err(AppError::Invalid);
    }
    Ok(Some(deduped))
}

/// Crée une clé nommée avec portée complète du rôle ; elle n'est jamais relisible ensuite.
pub async fn issue(pool: &SqlitePool, name: &str, role: Role) -> AppResult<IssuedKey> {
    issue_with_scopes(pool, name, role, None).await
}

/// Crée une clé nommée avec une portée explicite (ou complète si `None`).
pub async fn issue_with_scopes(
    pool: &SqlitePool,
    name: &str,
    role: Role,
    scopes: Option<&[Permission]>,
) -> AppResult<IssuedKey> {
    issue_with_executor(pool, name, role, scopes).await
}

/// Variante générique sur exécuteur SQLx (pool ou transaction SQL).
pub async fn issue_with_executor<'e, E>(
    executor: E,
    name: &str,
    role: Role,
    scopes: Option<&[Permission]>,
) -> AppResult<IssuedKey>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let name = validate_name(name)?;
    let validated_scopes = validate_scopes(role, scopes)?;
    let id = random_hex(KEY_ID_BYTES);
    let secret = random_hex(KEY_SECRET_BYTES);
    let record = repository::insert_api_key(
        executor,
        &id,
        &name,
        &sha256_hex(&secret),
        role,
        validated_scopes.as_deref(),
    )
    .await?;
    Ok(IssuedKey {
        record,
        raw: format!("{KEY_PREFIX}{id}_{secret}"),
    })
}

/// Crée la clé d'un worker qui s'enrôle et révoque celles qu'il portait sous ce nom : une seule clé
/// active par worker. À appeler dans une transaction.
pub async fn issue_for_worker(
    connection: &mut sqlx::SqliteConnection,
    worker_name: &str,
) -> AppResult<IssuedKey> {
    repository::revoke_worker_keys(&mut *connection, worker_name).await?;
    issue_with_executor(&mut *connection, worker_name, Role::Worker, None).await
}

/// Installe une clé dont la valeur est imposée (tests, banc d'essai) ; le format doit être valide.
pub async fn install_known_key(
    pool: &SqlitePool,
    raw: &str,
    name: &str,
    role: Role,
) -> AppResult<ApiKeyRecord> {
    let (id, secret) = parse(raw).ok_or(AppError::Invalid)?;
    repository::insert_api_key(
        pool,
        id,
        &validate_name(name)?,
        &sha256_hex(secret),
        role,
        None,
    )
    .await
}

/// Clé non révoquée correspondant à la valeur présentée ; la dernière utilisation est mise à jour.
pub async fn authenticate(pool: &SqlitePool, raw: &str) -> AppResult<Option<ApiKeyRecord>> {
    let Some((id, secret)) = parse(raw) else {
        return Ok(None);
    };
    let Some((record, stored_hash)) = repository::find_active_api_key(pool, id).await? else {
        return Ok(None);
    };
    let matches = sha256_hex(secret).as_bytes().ct_eq(stored_hash.as_bytes());
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
    fn test_name_validation() -> AppResult<()> {
        assert_eq!(validate_name("  Worker NAS ")?, "Worker NAS");
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(KEY_NAME_MAX_LEN + 1)).is_err());
        assert!(validate_name("tab\there").is_err());
        Ok(())
    }
}
