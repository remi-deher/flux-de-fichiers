//! Envoi de courriels par SMTP (ADR 0015). Le transport est derrière un trait : les tests
//! enregistrent les messages au lieu de joindre un relais.

use crate::{
    config::{SmtpConfig, SmtpTls},
    error::{AppError, AppResult},
};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};
use std::{future::Future, time::Duration};

/// Délai maximal d'une conversation avec le relais.
const SMTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Message texte à envoyer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingMail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

pub trait MailSender {
    fn send(&self, mail: OutgoingMail) -> impl Future<Output = AppResult<()>> + Send;
}

/// Envoi réel par le relais configuré.
pub struct SmtpSender<'a>(pub &'a SmtpConfig);

fn mailbox(address: &str) -> AppResult<Mailbox> {
    address.parse().map_err(|_| AppError::Invalid)
}

fn build_message(from: &str, mail: &OutgoingMail) -> AppResult<Message> {
    Message::builder()
        .from(mailbox(from)?)
        .to(mailbox(&mail.to)?)
        .subject(&mail.subject)
        .body(mail.body.clone())
        .map_err(|_| AppError::Invalid)
}

fn transport(config: &SmtpConfig) -> AppResult<AsyncSmtpTransport<Tokio1Executor>> {
    let builder = match config.tls {
        SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host),
        SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host),
        SmtpTls::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            &config.host,
        )),
    }
    .map_err(|_| AppError::Internal)?
    .port(config.port)
    .timeout(Some(SMTP_TIMEOUT));
    Ok(match (&config.username, &config.password) {
        (Some(user), Some(password)) => {
            builder.credentials(Credentials::new(user.clone(), password.clone()))
        }
        _ => builder,
    }
    .build())
}

impl MailSender for SmtpSender<'_> {
    async fn send(&self, mail: OutgoingMail) -> AppResult<()> {
        let message = build_message(&self.0.from, &mail)?;
        transport(self.0)?.send(message).await.map_err(|error| {
            tracing::warn!(error = %error, "smtp send failed");
            AppError::Internal
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(to: &str) -> OutgoingMail {
        OutgoingMail {
            to: to.into(),
            subject: "Sujet".into(),
            body: "Corps".into(),
        }
    }

    fn config(tls: SmtpTls) -> SmtpConfig {
        SmtpConfig {
            host: "smtp.exemple.fr".into(),
            port: tls.default_port(),
            tls,
            username: Some("u".into()),
            password: Some("p".into()),
            from: "Flux <flux@exemple.fr>".into(),
        }
    }

    #[test]
    fn test_message_is_built_from_valid_addresses() {
        assert!(build_message("flux@exemple.fr", &mail("remi@exemple.fr")).is_ok());
        assert!(build_message("Flux <flux@exemple.fr>", &mail("remi@exemple.fr")).is_ok());
    }

    #[test]
    fn test_message_refuses_invalid_addresses() {
        assert!(build_message("pas une adresse", &mail("remi@exemple.fr")).is_err());
        assert!(build_message("flux@exemple.fr", &mail("remi")).is_err());
    }

    #[tokio::test]
    async fn test_every_tls_mode_builds_a_transport() {
        for tls in [SmtpTls::StartTls, SmtpTls::Tls, SmtpTls::None] {
            assert!(transport(&config(tls)).is_ok());
        }
    }
}
