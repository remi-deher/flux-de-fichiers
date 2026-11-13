//! Comptes, clés d'API et réglages d'instance.

use crate::{
    error::{AppError, AppResult},
    node_view::AccountCopy,
};
use flux_shared::Role;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

/// Clé du réglage d'instance qui porte le nom de la centrale.
pub const INSTANCE_NAME_KEY: &str = "instance_name";

/// Un compte humain. Le haché du mot de passe n'en fait jamais partie.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Account {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
}

#[derive(FromRow)]
struct AccountRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    disabled: bool,
}

impl AccountRow {
    fn into_parts(self) -> AppResult<(Account, String)> {
        let role = Role::parse(&self.role).ok_or(AppError::Internal)?;
        Ok((
            Account {
                id: self.id,
                username: self.username,
                role,
                disabled: self.disabled,
            },
            self.password_hash,
        ))
    }
}

#[derive(FromRow)]
struct AccountCopyRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    disabled: bool,
    version: i64,
}

impl AccountRow {
    fn into_account(self) -> AppResult<Account> {
        self.into_parts().map(|(account, _)| account)
    }
}

/// Comptes humains, par identifiant, sans leur haché.
pub async fn list_accounts(pool: &SqlitePool) -> AppResult<Vec<Account>> {
    sqlx::query_as::<_, AccountRow>(
        "SELECT id, username, password_hash, role, disabled FROM accounts ORDER BY username",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(AccountRow::into_account)
    .collect()
}

/// Crée un compte ; un identifiant déjà pris donne `Conflict`.
pub async fn create_account(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    role: Role,
) -> AppResult<Account> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO accounts(id, username, password_hash, role) VALUES (?1, ?2, ?3, ?4)")
        .bind(&id)
        .bind(username)
        .bind(password_hash)
        .bind(role.as_str())
        .execute(pool)
        .await?;
    Ok(Account {
        id,
        username: username.to_owned(),
        role,
        disabled: false,
    })
}

/// Change le rôle et/ou l'état d'un compte, sans jamais laisser la centrale sans administrateur actif.
/// La version du compte augmente et ses sessions sont fermées : les droits changent tout de suite.
pub async fn update_account(
    pool: &SqlitePool,
    id: &str,
    role: Option<Role>,
    disabled: Option<bool>,
) -> AppResult<Account> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as::<_, AccountRow>(
        "SELECT id, username, password_hash, role, disabled FROM accounts WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    let mut account = row.into_account()?;
    account.role = role.unwrap_or(account.role);
    account.disabled = disabled.unwrap_or(account.disabled);
    let active_admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM accounts WHERE role = 'admin' AND disabled = 0 AND id != ?1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let stays_active_admin = account.role == Role::Admin && !account.disabled;
    if active_admins == 0 && !stays_active_admin {
        return Err(AppError::Conflict);
    }
    sqlx::query(
        "UPDATE accounts SET role = ?1, disabled = ?2, version = version + 1, \
         updated_at = datetime('now') WHERE id = ?3",
    )
    .bind(account.role.as_str())
    .bind(account.disabled)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM sessions WHERE account_id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(account)
}

/// Tous les comptes avec leur haché et leur version, pour la copie vers les workers.
pub async fn list_account_copies(pool: &SqlitePool) -> AppResult<Vec<AccountCopy>> {
    sqlx::query_as::<_, AccountCopyRow>(
        "SELECT id, username, password_hash, role, disabled, version FROM accounts ORDER BY username",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(AccountCopy {
            id: row.id,
            username: row.username,
            password_hash: row.password_hash,
            role: Role::parse(&row.role).ok_or(AppError::Internal)?,
            disabled: row.disabled,
            version: row.version,
        })
    })
    .collect()
}

/// Clé d'API telle que listée : jamais son secret ni son haché.
#[derive(Clone, Debug, PartialEq, Eq, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

pub async fn account_count(pool: &SqlitePool) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(pool)
        .await?)
}

/// Crée le premier compte, une seule fois : `None` si un compte existe déjà (installation terminée).
pub async fn create_first_account(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    role: Role,
) -> AppResult<Option<Account>> {
    let id = Uuid::new_v4().to_string();
    let inserted = sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role) \
         SELECT ?1, ?2, ?3, ?4 WHERE NOT EXISTS (SELECT 1 FROM accounts)",
    )
    .bind(&id)
    .bind(username)
    .bind(password_hash)
    .bind(role.as_str())
    .execute(pool)
    .await?
    .rows_affected();
    Ok((inserted == 1).then_some(Account {
        id,
        username: username.to_owned(),
        role,
        disabled: false,
    }))
}

pub async fn find_account_by_username(
    pool: &SqlitePool,
    username: &str,
) -> AppResult<Option<(Account, String)>> {
    sqlx::query_as::<_, AccountRow>(
        "SELECT id, username, password_hash, role, disabled FROM accounts WHERE username = ?1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?
    .map(AccountRow::into_parts)
    .transpose()
}

pub async fn get_account(pool: &SqlitePool, id: &str) -> AppResult<Option<(Account, String)>> {
    sqlx::query_as::<_, AccountRow>(
        "SELECT id, username, password_hash, role, disabled FROM accounts WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(AccountRow::into_parts)
    .transpose()
}

/// Remplace le haché ; la version du compte augmente et ses autres sessions sont fermées.
pub async fn update_password(
    pool: &SqlitePool,
    account_id: &str,
    password_hash: &str,
    keep_session_hash: Option<&str>,
) -> AppResult<()> {
    let updated = sqlx::query(
        "UPDATE accounts SET password_hash = ?1, version = version + 1, updated_at = datetime('now') WHERE id = ?2",
    )
    .bind(password_hash)
    .bind(account_id)
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM sessions WHERE account_id = ?1 AND token_hash != ?2")
        .bind(account_id)
        .bind(keep_session_hash.unwrap_or_default())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT value FROM instance_settings WHERE key = ?1")
            .bind(key)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO instance_settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = ?2",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_api_key(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    key_hash: &str,
    role: Role,
) -> AppResult<ApiKeyRecord> {
    Ok(sqlx::query_as::<_, ApiKeyRecord>(
        "INSERT INTO api_keys (id, name, key_hash, role) VALUES (?1, ?2, ?3, ?4) \
         RETURNING id, name, role, created_at, last_used_at, revoked_at",
    )
    .bind(id)
    .bind(name)
    .bind(key_hash)
    .bind(role.as_str())
    .fetch_one(pool)
    .await?)
}

pub async fn list_api_keys(pool: &SqlitePool) -> AppResult<Vec<ApiKeyRecord>> {
    Ok(sqlx::query_as::<_, ApiKeyRecord>(
        "SELECT id, name, role, created_at, last_used_at, revoked_at FROM api_keys \
         ORDER BY revoked_at IS NOT NULL, created_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// Clé non révoquée et son haché, pour la vérification.
pub async fn find_active_api_key(
    pool: &SqlitePool,
    id: &str,
) -> AppResult<Option<(ApiKeyRecord, String)>> {
    #[derive(FromRow)]
    struct Row {
        id: String,
        name: String,
        role: String,
        created_at: String,
        last_used_at: Option<String>,
        revoked_at: Option<String>,
        key_hash: String,
    }
    Ok(sqlx::query_as::<_, Row>(
        "SELECT id, name, role, created_at, last_used_at, revoked_at, key_hash \
         FROM api_keys WHERE id = ?1 AND revoked_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(|row| {
        (
            ApiKeyRecord {
                id: row.id,
                name: row.name,
                role: row.role,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                revoked_at: row.revoked_at,
            },
            row.key_hash,
        )
    }))
}

/// Délai minimal entre deux mises à jour de la dernière utilisation d'une clé.
const LAST_USED_REFRESH_SQL: &str = "-60 seconds";

pub async fn touch_api_key(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE api_keys SET last_used_at = datetime('now') \
         WHERE id = ?1 AND (last_used_at IS NULL OR last_used_at < datetime('now', ?2))",
    )
    .bind(id)
    .bind(LAST_USED_REFRESH_SQL)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_api_key(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let revoked = sqlx::query(
        "UPDATE api_keys SET revoked_at = datetime('now') WHERE id = ?1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    if revoked == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}
