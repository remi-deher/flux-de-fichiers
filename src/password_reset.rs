//! Mot de passe oublié par courriel (ADR 0015) : jeton à usage unique, haché en base, envoyé en lien.

use crate::{
    api_keys::hash_secret,
    error::{AppError, AppResult},
    mail::{MailSender, OutgoingMail},
    password, repository,
};
use chacha20poly1305::aead::{OsRng, rand_core::RngCore};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// Durée de vie d'un jeton, en secondes.
pub const RESET_TOKEN_LIFETIME_SECONDS: i64 = 1_800;
const SECONDS_PER_MINUTE: i64 = 60;
/// Délai minimal entre deux courriels pour un même compte.
const REQUEST_COOLDOWN_SECONDS: i64 = 60;
const RESET_TOKEN_BYTES: usize = 32;
const EMAIL_MAX_LEN: usize = 254;
/// Demandes tolérées par adresse saisie et par fenêtre.
const MAX_REQUESTS_PER_WINDOW: usize = 5;
const REQUEST_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Adresses suivies au plus : au-delà, les demandes sont refusées jusqu'à l'oubli des plus anciennes.
const MAX_TRACKED_ADDRESSES: usize = 10_000;
/// Début du fragment d'adresse qui porte le jeton (lu par le client).
pub const RESET_FRAGMENT_PREFIX: &str = "reset=";

/// Adresse normalisée (espaces retirés) : une seule `@`, un domaine avec un point, sans espace.
pub fn validate_email(raw: &str) -> AppResult<String> {
    let email = raw.trim();
    let valid = email.len() <= EMAIL_MAX_LEN
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if valid {
        Ok(email.to_owned())
    } else {
        Err(AppError::Invalid)
    }
}

/// Adresse saisie dans un formulaire : vide, elle efface (`None`).
pub fn parse_optional_email(raw: &str) -> AppResult<Option<String>> {
    if raw.trim().is_empty() {
        Ok(None)
    } else {
        validate_email(raw).map(Some)
    }
}

fn new_token() -> String {
    let mut bytes = [0u8; RESET_TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Limite les demandes par adresse saisie, que le compte existe ou non.
#[derive(Default)]
pub struct RequestLimiter {
    requests: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RequestLimiter {
    /// Enregistre une demande à l'instant `now` ; `RateLimited` si la fenêtre est pleine.
    pub fn admit(&self, key: &str, now: Instant) -> AppResult<()> {
        let mut requests = self.requests.lock().map_err(|_| AppError::Internal)?;
        requests.retain(|_, times| {
            times.retain(|time| now.duration_since(*time) < REQUEST_WINDOW);
            !times.is_empty()
        });
        let key = key.to_lowercase();
        if !requests.contains_key(&key) && requests.len() >= MAX_TRACKED_ADDRESSES {
            return Err(AppError::RateLimited);
        }
        let times = requests.entry(key).or_default();
        if times.len() >= MAX_REQUESTS_PER_WINDOW {
            return Err(AppError::RateLimited);
        }
        times.push(now);
        Ok(())
    }
}

static LIMITER: LazyLock<RequestLimiter> = LazyLock::new(RequestLimiter::default);

/// Valide l'adresse d'une demande et la compte dans la limite ; renvoie l'adresse normalisée.
pub fn admit_request(raw: &str) -> AppResult<String> {
    let email = validate_email(raw)?;
    LIMITER.admit(&email, Instant::now())?;
    Ok(email)
}

/// Lien envoyé par courriel : le jeton est dans le fragment, qui ne quitte pas le navigateur.
pub fn reset_link(origin: &str, token: &str) -> String {
    format!("{origin}/#{RESET_FRAGMENT_PREFIX}{token}")
}

fn reset_mail(to: &str, link: &str) -> OutgoingMail {
    OutgoingMail {
        to: to.to_owned(),
        subject: "Réinitialisation de votre mot de passe Flux de Fichiers".into(),
        body: format!(
            "Une réinitialisation du mot de passe a été demandée pour ce compte.\n\n\
             Choisissez un nouveau mot de passe : {link}\n\n\
             Ce lien est valable {} minutes et ne sert qu'une fois. \
             Si vous n'êtes pas à l'origine de cette demande, ignorez ce message.\n",
            RESET_TOKEN_LIFETIME_SECONDS / SECONDS_PER_MINUTE
        ),
    }
}

/// Crée un jeton pour le compte qui porte cette adresse et lui envoie le lien. Sans compte actif, ou
/// si un courriel vient de partir, ne fait rien : l'appelant répond pareil dans tous les cas. Un échec
/// d'envoi est journalisé mais jamais renvoyé.
pub async fn issue_and_send(
    pool: &SqlitePool,
    mailer: &impl MailSender,
    origin: &str,
    email: &str,
) -> AppResult<()> {
    repository::purge_expired_reset_tokens(pool).await?;
    let Some(account) = repository::find_account_by_email(pool, email)
        .await?
        .filter(|account| !account.disabled)
    else {
        tracing::info!("password reset requested for an unknown or disabled address");
        return Ok(());
    };
    if repository::count_recent_reset_tokens(pool, &account.id, REQUEST_COOLDOWN_SECONDS).await? > 0
    {
        tracing::info!(account = %account.id, "password reset skipped: cooldown");
        return Ok(());
    }
    repository::delete_account_reset_tokens(pool, &account.id).await?;
    let token = new_token();
    repository::insert_reset_token(
        pool,
        &hash_secret(&token),
        &account.id,
        RESET_TOKEN_LIFETIME_SECONDS,
    )
    .await?;
    match mailer
        .send(reset_mail(email, &reset_link(origin, &token)))
        .await
    {
        Ok(()) => tracing::info!(account = %account.id, "password reset mail sent"),
        Err(error) => {
            tracing::warn!(account = %account.id, error = %error, "password reset mail failed");
        }
    }
    Ok(())
}

/// Change le mot de passe avec un jeton valide, qui est consommé ; ferme toutes les sessions du compte.
/// Le mot de passe est vérifié avant le jeton : un mot de passe refusé ne le brûle pas.
pub async fn confirm_reset(pool: &SqlitePool, token: &str, new_password: &str) -> AppResult<()> {
    password::validate_password(new_password)?;
    let account_id = repository::consume_reset_token(pool, &hash_secret(token.trim()))
        .await?
        .ok_or(AppError::Invalid)?;
    let (account, _) = repository::get_account(pool, &account_id)
        .await?
        .filter(|(account, _)| !account.disabled)
        .ok_or(AppError::Invalid)?;
    let hash = password::hash(new_password.to_owned()).await?;
    repository::update_password(pool, &account.id, &hash, None).await?;
    repository::delete_account_reset_tokens(pool, &account.id).await?;
    tracing::info!(account = %account.id, "password reset completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_validation() -> AppResult<()> {
        assert_eq!(validate_email("  Remi@Exemple.fr ")?, "Remi@Exemple.fr");
        for bad in [
            "",
            "sans-arobase",
            "@exemple.fr",
            "a@b",
            "a@@b.fr",
            "a b@c.fr",
            "a@.fr",
            "a@b.",
        ] {
            assert!(validate_email(bad).is_err(), "{bad}");
        }
        assert!(validate_email(&format!("{}@b.fr", "x".repeat(EMAIL_MAX_LEN))).is_err());
        Ok(())
    }

    #[test]
    fn test_empty_optional_email_clears() -> AppResult<()> {
        assert_eq!(parse_optional_email("   ")?, None);
        assert_eq!(parse_optional_email("a@b.fr")?.as_deref(), Some("a@b.fr"));
        assert!(parse_optional_email("x").is_err());
        Ok(())
    }

    #[test]
    fn test_tokens_are_long_hex_and_distinct() {
        let token = new_token();
        assert_eq!(token.len(), RESET_TOKEN_BYTES * 2);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(token, new_token());
    }

    #[test]
    fn test_limiter_blocks_after_the_window_is_full_and_ignores_case() {
        let limiter = RequestLimiter::default();
        let start = Instant::now();
        for _ in 0..MAX_REQUESTS_PER_WINDOW {
            assert!(limiter.admit("a@b.fr", start).is_ok());
        }
        assert!(matches!(
            limiter.admit("A@B.FR", start),
            Err(AppError::RateLimited)
        ));
        assert!(limiter.admit("autre@b.fr", start).is_ok());
        assert!(limiter.admit("a@b.fr", start + REQUEST_WINDOW).is_ok());
    }

    #[test]
    fn test_link_keeps_the_token_in_the_fragment() {
        assert_eq!(
            reset_link("https://flux.exemple.fr", "abc"),
            "https://flux.exemple.fr/#reset=abc"
        );
    }

    #[test]
    fn test_mail_names_the_validity_and_carries_the_link() {
        let mail = reset_mail("a@b.fr", "https://x/#reset=t");
        assert_eq!(mail.to, "a@b.fr");
        assert!(mail.body.contains("https://x/#reset=t"));
        assert!(mail.body.contains("30 minutes"));
    }
}
