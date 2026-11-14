use serde::{Deserialize, Serialize};
use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

const SMTP_STARTTLS_PORT: u16 = 587;
const SMTP_TLS_PORT: u16 = 465;
const SMTP_PLAIN_PORT: u16 = 25;
const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:3000";
const DEFAULT_WEB_SESSION_TTL_SECONDS: i64 = 28_800;
const MAX_WEB_SESSION_TTL_SECONDS: i64 = 2_592_000;

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub input_dir: PathBuf,
    pub config_dir: PathBuf,
    pub tools_cache_dir: PathBuf,
    pub plugins_dir: Option<PathBuf>,
    pub static_dir: PathBuf,
    pub max_file_bytes: u64,
    pub local_worker: bool,
    pub web_origin: Option<String>,
    pub web_session_ttl_seconds: i64,
    pub tls: Option<(PathBuf, PathBuf)>,
    /// Relais d'envoi des courriels ; absent, le mot de passe oublié par courriel est désactivé.
    pub smtp: Option<SmtpConfig>,
}

/// Chiffrement de la connexion au relais SMTP (`SMTP_TLS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTls {
    /// Connexion en clair puis passage en TLS (port 587 par défaut).
    #[serde(rename = "starttls")]
    StartTls,
    /// TLS dès la connexion (port 465 par défaut).
    Tls,
    /// Aucun chiffrement (port 25 par défaut) : à réserver à un relais local.
    None,
}

impl SmtpTls {
    pub const fn default_port(self) -> u16 {
        match self {
            Self::StartTls => SMTP_STARTTLS_PORT,
            Self::Tls => SMTP_TLS_PORT,
            Self::None => SMTP_PLAIN_PORT,
        }
    }

    fn parse(raw: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_value(serde_json::Value::String(
            raw.trim().to_lowercase(),
        ))?)
    }
}

/// Réglages SMTP. Le mot de passe vient d'un fichier ou de l'environnement, jamais de la base.
#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub tls: SmtpTls,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
}

impl SmtpConfig {
    /// Lit `SMTP_*` ; `None` si `SMTP_HOST` n'est pas défini.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let password =
            if env::var("SMTP_PASSWORD").is_ok() || env::var("SMTP_PASSWORD_FILE").is_ok() {
                Some(value("SMTP_PASSWORD")?)
            } else {
                None
            };
        Self::from_variables(|name| env::var(name).ok(), password)
    }

    fn from_variables(
        variable: impl Fn(&str) -> Option<String>,
        password: Option<String>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(host) = variable("SMTP_HOST").filter(|host| !host.trim().is_empty()) else {
            return Ok(None);
        };
        let tls = variable("SMTP_TLS")
            .map(|raw| SmtpTls::parse(&raw))
            .transpose()?
            .unwrap_or(SmtpTls::StartTls);
        let port = variable("SMTP_PORT")
            .map(|raw| raw.trim().parse())
            .transpose()?
            .unwrap_or_else(|| tls.default_port());
        let from = variable("SMTP_FROM")
            .filter(|from| !from.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("SMTP_FROM is required with SMTP_HOST"))?;
        let username = variable("SMTP_USERNAME").filter(|name| !name.is_empty());
        anyhow::ensure!(
            username.is_some() == password.is_some(),
            "SMTP_USERNAME and SMTP_PASSWORD must be supplied together"
        );
        Ok(Some(Self {
            host: host.trim().to_owned(),
            port,
            tls,
            username,
            password,
            from: from.trim().to_owned(),
        }))
    }
}

pub fn value(name: &str) -> anyhow::Result<String> {
    Ok(if let Ok(path) = env::var(format!("{name}_FILE")) {
        std::fs::read_to_string(path)?.trim().to_owned()
    } else {
        env::var(name)?
    })
}

pub fn secret(name: &str) -> anyhow::Result<String> {
    let value = value(name)?;
    anyhow::ensure!(value.len() >= 32, "{name} must have at least 32 characters");
    Ok(value)
}

/// Dossier de configuration persistant (`/config` en conteneur).
pub fn resolve_config_dir() -> PathBuf {
    if let Ok(dir) = env::var("CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    let default_dir = PathBuf::from("/config");
    if default_dir.is_dir() || std::fs::create_dir_all(&default_dir).is_ok() {
        default_dir
    } else {
        PathBuf::from("var/config")
    }
}

/// Adresse d'écoute du port HTTP unique, la même pour le central et le worker.
pub fn bind_address() -> anyhow::Result<SocketAddr> {
    let raw = env::var("BIND_ADDR")
        .or_else(|_| env::var("BIND"))
        .unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.into());
    Ok(raw.parse()?)
}

pub fn central_url(raw: &str) -> anyhow::Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    anyhow::ensure!(
        url.scheme() == "https" || url.scheme() == "http",
        "Central URL requires HTTP or HTTPS"
    );
    anyhow::ensure!(
        url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "Central URL must be an origin without credentials"
    );
    Ok(url)
}

impl Config {
    /// Dossier qui reçoit une copie locale de chaque dépôt de plugins synchronisé.
    pub fn plugin_repos_dir(&self) -> PathBuf {
        self.config_dir.join("plugins")
    }

    pub fn load() -> anyhow::Result<Self> {
        let config_dir = resolve_config_dir();
        std::fs::create_dir_all(&config_dir)?;

        let bind = bind_address()?;

        let tls = match (env::var("TLS_CERT"), env::var("TLS_KEY")) {
            (Ok(cert), Ok(key)) => Some((cert.into(), key.into())),
            (Err(_), Err(_)) => None,
            _ => anyhow::bail!("TLS_CERT and TLS_KEY must be supplied together"),
        };

        let max_file_bytes = env::var("MAX_FILE_BYTES")
            .unwrap_or_else(|_| "536870912".into())
            .parse()?;
        anyhow::ensure!(
            max_file_bytes > 0 && max_file_bytes <= i64::MAX as u64,
            "MAX_FILE_BYTES must be positive"
        );

        let web_origin = if let Ok(val) = env::var("WEB_ORIGIN") {
            let parsed_origin = reqwest::Url::parse(&val)?;
            anyhow::ensure!(
                (parsed_origin.scheme() == "https" || parsed_origin.scheme() == "http")
                    && parsed_origin.host_str().is_some()
                    && parsed_origin.path() == "/"
                    && parsed_origin.query().is_none()
                    && parsed_origin.fragment().is_none()
                    && parsed_origin.username().is_empty()
                    && parsed_origin.password().is_none()
                    && parsed_origin.origin().ascii_serialization() == val,
                "WEB_ORIGIN must be a canonical origin without path or credentials"
            );
            Some(val)
        } else {
            None
        };

        let web_session_ttl_seconds = env::var("WEB_SESSION_TTL_SECONDS")
            .unwrap_or_else(|_| DEFAULT_WEB_SESSION_TTL_SECONDS.to_string())
            .parse()?;
        anyhow::ensure!(
            (60..=MAX_WEB_SESSION_TTL_SECONDS).contains(&web_session_ttl_seconds),
            "WEB_SESSION_TTL_SECONDS must be between 60 and 2592000"
        );

        let static_dir: PathBuf = env::var("STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let default_path = PathBuf::from("/usr/share/flux-de-fichiers/frontend");
                if default_path.is_dir() {
                    default_path
                } else if PathBuf::from("frontend/dist").is_dir() {
                    PathBuf::from("frontend/dist")
                } else {
                    default_path
                }
            });

        let database_url = if let Ok(url) = value("DATABASE_URL") {
            url
        } else {
            format!("sqlite://{}", config_dir.join("flux.db").display())
        };

        let data_dir: PathBuf = env::var("DATA_DIR").unwrap_or_else(|_| "var".into()).into();

        Ok(Self {
            bind,
            tls,
            max_file_bytes,
            local_worker: env::var("LOCAL_WORKER")
                .map(|value| value != "false" && value != "0")
                .unwrap_or(true),
            web_origin,
            web_session_ttl_seconds,
            database_url,
            data_dir: data_dir.clone(),
            input_dir: env::var("INPUT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    if Path::new("/input").is_dir() {
                        PathBuf::from("/input")
                    } else if data_dir.is_absolute() {
                        data_dir.join("input")
                    } else {
                        PathBuf::from("var/input")
                    }
                }),
            config_dir,
            tools_cache_dir: env::var("TOOLS_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| data_dir.join("tools-cache")),
            plugins_dir: env::var("PLUGINS_DIR").map(PathBuf::from).ok().or_else(|| {
                let container_path = PathBuf::from("/plugins");
                if container_path.is_dir() {
                    Some(container_path)
                } else {
                    let dev_path = PathBuf::from("../flux-de-fichiers-plugins");
                    if dev_path.is_dir() {
                        Some(dev_path)
                    } else {
                        None
                    }
                }
            }),
            static_dir,
            smtp: SmtpConfig::from_env()?,
        })
    }

    /// Le lien de réinitialisation peut-il être envoyé : relais SMTP et adresse publique connus ?
    pub fn password_reset_enabled(&self) -> bool {
        self.smtp.is_some() && self.web_origin.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn smtp(vars: &[(&str, &str)], password: Option<&str>) -> anyhow::Result<Option<SmtpConfig>> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        SmtpConfig::from_variables(|name| map.get(name).cloned(), password.map(str::to_owned))
    }

    #[test]
    fn test_smtp_is_disabled_without_a_host() -> anyhow::Result<()> {
        assert!(smtp(&[("SMTP_FROM", "a@b.fr")], None)?.is_none());
        Ok(())
    }

    #[test]
    fn test_smtp_defaults_follow_the_tls_mode() -> anyhow::Result<()> {
        let config = smtp(
            &[("SMTP_HOST", "smtp.exemple.fr"), ("SMTP_FROM", "a@b.fr")],
            None,
        )?
        .ok_or_else(|| anyhow::anyhow!("absent"))?;
        assert_eq!((config.tls, config.port), (SmtpTls::StartTls, 587));
        let implicit = smtp(
            &[
                ("SMTP_HOST", "h"),
                ("SMTP_FROM", "a@b.fr"),
                ("SMTP_TLS", "TLS"),
            ],
            None,
        )?
        .ok_or_else(|| anyhow::anyhow!("absent"))?;
        assert_eq!((implicit.tls, implicit.port), (SmtpTls::Tls, 465));
        let plain = smtp(
            &[
                ("SMTP_HOST", "h"),
                ("SMTP_FROM", "a@b.fr"),
                ("SMTP_TLS", "none"),
                ("SMTP_PORT", "2525"),
            ],
            None,
        )?
        .ok_or_else(|| anyhow::anyhow!("absent"))?;
        assert_eq!((plain.tls, plain.port), (SmtpTls::None, 2525));
        Ok(())
    }

    #[test]
    fn test_smtp_rejects_incomplete_or_unknown_settings() {
        assert!(smtp(&[("SMTP_HOST", "h")], None).is_err());
        assert!(
            smtp(
                &[
                    ("SMTP_HOST", "h"),
                    ("SMTP_FROM", "a@b.fr"),
                    ("SMTP_TLS", "ssl")
                ],
                None
            )
            .is_err()
        );
        assert!(
            smtp(
                &[
                    ("SMTP_HOST", "h"),
                    ("SMTP_FROM", "a@b.fr"),
                    ("SMTP_PORT", "x")
                ],
                None
            )
            .is_err()
        );
        assert!(
            smtp(
                &[
                    ("SMTP_HOST", "h"),
                    ("SMTP_FROM", "a@b.fr"),
                    ("SMTP_USERNAME", "u")
                ],
                None
            )
            .is_err()
        );
        assert!(
            smtp(
                &[("SMTP_HOST", "h"), ("SMTP_FROM", "a@b.fr")],
                Some("secret")
            )
            .is_err()
        );
    }

    #[test]
    fn test_smtp_accepts_credentials_together() -> anyhow::Result<()> {
        let config = smtp(
            &[
                ("SMTP_HOST", "h"),
                ("SMTP_FROM", "a@b.fr"),
                ("SMTP_USERNAME", "u"),
            ],
            Some("secret"),
        )?
        .ok_or_else(|| anyhow::anyhow!("absent"))?;
        assert_eq!(config.username.as_deref(), Some("u"));
        assert_eq!(config.password.as_deref(), Some("secret"));
        Ok(())
    }
}
