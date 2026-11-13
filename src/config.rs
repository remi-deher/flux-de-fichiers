use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

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
        })
    }
}
