use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub input_dir: PathBuf,
    pub admin_key: String,
    pub worker_key: String,
    pub max_file_bytes: u64,
    pub local_worker: bool,
    pub tls: Option<(PathBuf, PathBuf)>,
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

pub fn coordinator_url(raw: &str) -> anyhow::Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    anyhow::ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && local),
        "Coordinator requires HTTPS except on loopback"
    );
    anyhow::ensure!(
        url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "Coordinator URL must be an origin without credentials"
    );
    Ok(url)
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let bind: SocketAddr = env::var("BIND")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()?;
        let tls = match (env::var("TLS_CERT"), env::var("TLS_KEY")) {
            (Ok(cert), Ok(key)) => Some((cert.into(), key.into())),
            (Err(_), Err(_)) => None,
            _ => anyhow::bail!("TLS_CERT and TLS_KEY must be supplied together"),
        };
        anyhow::ensure!(
            bind.ip().is_loopback() || tls.is_some(),
            "Non-loopback listener requires TLS"
        );
        let admin_key = secret("ADMIN_KEY")?;
        let worker_key = secret("WORKER_KEY")?;
        anyhow::ensure!(
            admin_key != worker_key,
            "Admin and worker credentials must differ"
        );
        let max_file_bytes = env::var("MAX_FILE_BYTES")
            .unwrap_or_else(|_| "536870912".into())
            .parse()?;
        anyhow::ensure!(
            max_file_bytes > 0 && max_file_bytes <= i64::MAX as u64,
            "MAX_FILE_BYTES must be positive"
        );
        Ok(Self {
            bind,
            tls,
            admin_key,
            worker_key,
            max_file_bytes,
            local_worker: env::var("LOCAL_WORKER")
                .map(|value| value != "false" && value != "0")
                .unwrap_or(true),
            database_url: value("DATABASE_URL")?,
            data_dir: env::var("DATA_DIR").unwrap_or_else(|_| "var".into()).into(),
            input_dir: env::var("INPUT_DIR")
                .unwrap_or_else(|_| "var/input".into())
                .into(),
        })
    }
}
