//! Comptes, clés d'API et réglages d'instance.

use crate::{
    error::{AppError, AppResult},
    worker_view::AccountCopy,
};
pub use forgearr_shared::accounts::{Account, ApiKeyRecord};
use forgearr_shared::{Permission, Role};
use sqlx::SqlitePool;
use uuid::Uuid;

/// Clé du réglage d'instance qui porte le nom du central.
pub const INSTANCE_NAME_KEY: &str = "instance_name";

struct AccountRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    disabled: bool,
    email: Option<String>,
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
                email: self.email,
            },
            self.password_hash,
        ))
    }
}

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
    sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!: String", username, password_hash, role, disabled AS "disabled: bool", email FROM accounts ORDER BY username"#,
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
    email: Option<&str>,
) -> AppResult<Account> {
    let id = Uuid::new_v4().to_string();
    let role_str = role.as_str();
    sqlx::query!(
        "INSERT INTO accounts(id, username, password_hash, role, email) VALUES (?1, ?2, ?3, ?4, ?5)",
        id,
        username,
        password_hash,
        role_str,
        email,
    )
    .execute(pool)
    .await?;
    Ok(Account {
        id,
        username: username.to_owned(),
        role,
        disabled: false,
        email: email.map(str::to_owned),
    })
}

/// Change le rôle et/ou l'état d'un compte, sans jamais laisser le central sans administrateur actif.
/// La version du compte augmente et ses sessions sont fermées : les droits changent tout de suite.
pub async fn update_account(
    pool: &SqlitePool,
    id: &str,
    role: Option<Role>,
    disabled: Option<bool>,
) -> AppResult<Account> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!: String", username, password_hash, role, disabled AS "disabled: bool", email FROM accounts WHERE id = ?1"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    let mut account = row.into_account()?;
    account.role = role.unwrap_or(account.role);
    account.disabled = disabled.unwrap_or(account.disabled);
    let active_admins: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM accounts WHERE role = 'admin' AND disabled = 0 AND id != ?1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    let stays_active_admin = account.role == Role::Admin && !account.disabled;
    if active_admins == 0 && !stays_active_admin {
        return Err(AppError::Conflict);
    }
    let role_str = account.role.as_str();
    sqlx::query!(
        "UPDATE accounts SET role = ?1, disabled = ?2, version = version + 1, \
         updated_at = datetime('now') WHERE id = ?3",
        role_str,
        account.disabled,
        id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("DELETE FROM sessions WHERE account_id = ?1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(account)
}

/// Tous les comptes avec leur haché et leur version, pour la copie vers les workers.
pub async fn list_account_copies(pool: &SqlitePool) -> AppResult<Vec<AccountCopy>> {
    sqlx::query_as!(
        AccountCopyRow,
        r#"SELECT id AS "id!: String", username, password_hash, role, disabled AS "disabled: bool", version FROM accounts ORDER BY username"#,
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

/// Retrouve un compte par adresse e-mail (sans tenir compte de la casse).
pub async fn find_account_by_email(pool: &SqlitePool, email: &str) -> AppResult<Option<Account>> {
    sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!: String", username, password_hash, role, disabled AS "disabled: bool", email FROM accounts WHERE email = ?1 COLLATE NOCASE"#,
        email,
    )
    .fetch_optional(pool)
    .await?
    .map(AccountRow::into_account)
    .transpose()
}

/// Définit l'adresse d'un compte (`None` l'efface) ; une adresse déjà prise donne `Conflict`.
pub async fn set_account_email(pool: &SqlitePool, id: &str, email: Option<&str>) -> AppResult<()> {
    let updated = sqlx::query!(
        "UPDATE accounts SET email = ?1, updated_at = datetime('now') WHERE id = ?2",
        email,
        id,
    )
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn account_count(pool: &SqlitePool) -> AppResult<i64> {
    Ok(
        sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM accounts"#)
            .fetch_one(pool)
            .await?,
    )
}

/// Crée le premier compte, une seule fois : `None` si un compte existe déjà (installation terminée).
pub async fn create_first_account(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    role: Role,
    email: Option<&str>,
) -> AppResult<Option<Account>> {
    let id = Uuid::new_v4().to_string();
    let role_str = role.as_str();
    let inserted = sqlx::query!(
        "INSERT INTO accounts (id, username, password_hash, role, email) \
         SELECT ?1, ?2, ?3, ?4, ?5 WHERE NOT EXISTS (SELECT 1 FROM accounts)",
        id,
        username,
        password_hash,
        role_str,
        email,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok((inserted == 1).then_some(Account {
        id,
        username: username.to_owned(),
        role,
        disabled: false,
        email: email.map(str::to_owned),
    }))
}

pub async fn find_account_by_username(
    pool: &SqlitePool,
    username: &str,
) -> AppResult<Option<(Account, String)>> {
    sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!: String", username, password_hash, role, disabled AS "disabled: bool", email FROM accounts WHERE username = ?1"#,
        username,
    )
    .fetch_optional(pool)
    .await?
    .map(AccountRow::into_parts)
    .transpose()
}

pub async fn get_account(pool: &SqlitePool, id: &str) -> AppResult<Option<(Account, String)>> {
    sqlx::query_as!(
        AccountRow,
        r#"SELECT id AS "id!: String", username, password_hash, role, disabled AS "disabled: bool", email FROM accounts WHERE id = ?1"#,
        id,
    )
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
    let updated = sqlx::query!(
        "UPDATE accounts SET password_hash = ?1, version = version + 1, updated_at = datetime('now') WHERE id = ?2",
        password_hash,
        account_id,
    )
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    let keep_hash = keep_session_hash.unwrap_or_default();
    sqlx::query!(
        "DELETE FROM sessions WHERE account_id = ?1 AND token_hash != ?2",
        account_id,
        keep_hash,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar!("SELECT value FROM instance_settings WHERE key = ?1", key)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query!(
        "INSERT INTO instance_settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        key,
        value,
    )
    .execute(pool)
    .await?;
    Ok(())
}

struct ApiKeyRow {
    id: String,
    name: String,
    role: String,
    scopes: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    revoked_at: Option<String>,
}

impl ApiKeyRow {
    fn into_record(self) -> AppResult<ApiKeyRecord> {
        let scopes = match self.scopes {
            Some(raw) => Some(
                serde_json::from_str::<Vec<Permission>>(&raw).map_err(|_| AppError::Internal)?,
            ),
            None => None,
        };
        Ok(ApiKeyRecord {
            id: self.id,
            name: self.name,
            role: self.role,
            scopes,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
            revoked_at: self.revoked_at,
        })
    }
}

pub async fn insert_api_key<'e, E>(
    executor: E,
    id: &str,
    name: &str,
    key_hash: &str,
    role: Role,
    scopes: Option<&[Permission]>,
) -> AppResult<ApiKeyRecord>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let scopes_json = scopes
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| AppError::Invalid)?;
    let role_str = role.as_str();
    let row = sqlx::query_as!(
        ApiKeyRow,
        r#"INSERT INTO api_keys (id, name, key_hash, role, scopes) VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id AS "id!: String", name, role, scopes, created_at, last_used_at, revoked_at"#,
        id,
        name,
        key_hash,
        role_str,
        scopes_json,
    )
    .fetch_one(executor)
    .await?;
    row.into_record()
}

pub async fn list_api_keys(pool: &SqlitePool) -> AppResult<Vec<ApiKeyRecord>> {
    let rows = sqlx::query_as!(
        ApiKeyRow,
        r#"SELECT id AS "id!: String", name, role, scopes, created_at, last_used_at, revoked_at FROM api_keys ORDER BY revoked_at IS NOT NULL, created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(ApiKeyRow::into_record).collect()
}

/// Clé non révoquée et son haché, pour la vérification.
pub async fn find_active_api_key(
    pool: &SqlitePool,
    id: &str,
) -> AppResult<Option<(ApiKeyRecord, String)>> {
    let Some(row) = sqlx::query!(
        r#"SELECT id AS "id!: String", name, role, scopes, created_at, last_used_at, revoked_at, key_hash FROM api_keys WHERE id = ?1 AND revoked_at IS NULL"#,
        id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let key_hash = row.key_hash;
    let record = ApiKeyRow {
        id: row.id,
        name: row.name,
        role: row.role,
        scopes: row.scopes,
        created_at: row.created_at,
        last_used_at: row.last_used_at,
        revoked_at: row.revoked_at,
    }
    .into_record()?;
    Ok(Some((record, key_hash)))
}

/// Délai minimal entre deux mises à jour de la dernière utilisation d'une clé.
const LAST_USED_REFRESH_SQL: &str = "-60 seconds";

pub async fn touch_api_key(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query!(
        "UPDATE api_keys SET last_used_at = datetime('now') \
         WHERE id = ?1 AND (last_used_at IS NULL OR last_used_at < datetime('now', ?2))",
        id,
        LAST_USED_REFRESH_SQL,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Révoque les clés actives d'un worker, désigné par son nom.
pub async fn revoke_worker_keys<'e, E>(executor: E, worker_name: &str) -> AppResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "UPDATE api_keys SET revoked_at = datetime('now') \
         WHERE role = ?1 AND name = ?2 AND revoked_at IS NULL",
    )
    .bind(Role::Worker.as_str())
    .bind(worker_name)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn revoke_api_key(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let revoked = sqlx::query!(
        "UPDATE api_keys SET revoked_at = datetime('now') WHERE id = ?1 AND revoked_at IS NULL",
        id,
    )
    .execute(pool)
    .await?
    .rows_affected();
    if revoked == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}
