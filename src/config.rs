use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

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
    pub admin_key: String,
    pub worker_key: String,
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

fn resolve_config_dir() -> PathBuf {
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

fn bootstrap_secret(config_dir: &Path, env_name: &str, file_name: &str) -> anyhow::Result<String> {
    if env::var(env_name).is_ok() || env::var(format!("{env_name}_FILE")).is_ok() {
        return secret(env_name);
    }
    let key_path = config_dir.join(file_name);
    if key_path.is_file() {
        let val = std::fs::read_to_string(&key_path)?.trim().to_owned();
        if val.len() >= 32 {
            return Ok(val);
        }
    }
    let mut bytes = [0u8; 32];
    let mut urandom = std::fs::File::open("/dev/urandom")?;
    use std::io::Read;
    urandom.read_exact(&mut bytes)?;
    let generated: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    write_secure_file(&key_path, &generated)?;
    Ok(generated)
}

fn write_secure_file(path: &Path, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn coordinator_url(raw: &str) -> anyhow::Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw)?;
    anyhow::ensure!(
        url.scheme() == "https" || url.scheme() == "http",
        "Coordinator requires HTTP or HTTPS"
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
        let config_dir = resolve_config_dir();
        std::fs::create_dir_all(&config_dir)?;

        let bind_str = env::var("BIND_ADDR")
            .or_else(|_| env::var("BIND"))
            .unwrap_or_else(|_| "0.0.0.0:3000".into());
        let bind: SocketAddr = bind_str.parse()?;

        let tls = match (env::var("TLS_CERT"), env::var("TLS_KEY")) {
            (Ok(cert), Ok(key)) => Some((cert.into(), key.into())),
            (Err(_), Err(_)) => None,
            _ => anyhow::bail!("TLS_CERT and TLS_KEY must be supplied together"),
        };

        let admin_key = bootstrap_secret(&config_dir, "ADMIN_KEY", "admin_key")?;
        let mut worker_key = bootstrap_secret(&config_dir, "WORKER_KEY", "worker_key")?;
        if admin_key == worker_key {
            // If identical, regenerate a distinct worker key file
            let key_path = config_dir.join("worker_key");
            let mut bytes = [0u8; 32];
            let mut urandom = std::fs::File::open("/dev/urandom")?;
            use std::io::Read;
            urandom.read_exact(&mut bytes)?;
            worker_key = bytes.iter().map(|b| format!("{b:02x}")).collect();
            write_secure_file(&key_path, &worker_key)?;
        }
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
            admin_key,
            worker_key,
            max_file_bytes,
            local_worker: env::var("LOCAL_WORKER")
                .map(|value| value != "false" && value != "0")
                .unwrap_or(true),
            web_origin,
            web_session_ttl_seconds,
            database_url,
            data_dir: data_dir.clone(),
            input_dir: env::var("INPUT_DIR")
                .unwrap_or_else(|_| "var/input".into())
                .into(),
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
