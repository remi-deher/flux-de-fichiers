//! Transfert direct d'une source entre workers (ADR 0019) : serveur de lecture du worker source
//! et téléchargement vérifié côté worker traitant.

use crate::{error::AppError, library_scan, storage};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use forgearr_shared::transfer::{
    RedeemRequest, SourceTicket, TRANSFER_TICKET_HEADER, TransferGrant,
};
use futures_util::StreamExt;
use std::{future::Future, path::Path, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Semaphore,
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

/// Chemin de lecture de la source sur le serveur de transfert.
pub const TRANSFER_SOURCE_PATH: &str = "/transfer/source";
/// Transferts simultanés servis par un worker source.
const MAX_CONCURRENT_TRANSFERS: usize = 4;
/// Délai de connexion à la voie directe avant de basculer sur le repli par le central.
const DIRECT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Durée maximale d'une lecture directe, pour les très gros fichiers (6 heures).
const DIRECT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
/// Reprises d'une lecture directe interrompue, chacune avec un billet frais, avant le repli.
const DIRECT_RESUME_ATTEMPTS: u32 = 3;
/// Pause avant chaque reprise.
const DIRECT_RESUME_DELAY: Duration = Duration::from_secs(2);
/// Erreur d'un fichier reçu dont la taille ou l'empreinte ne correspond pas : jamais reprise.
pub const SOURCE_INTEGRITY: &str = "source_integrity";
const REDEEM_TIMEOUT: Duration = Duration::from_secs(10);

/// Valide un billet auprès du central ; `Err` porte le statut HTTP à renvoyer.
pub type RedeemFuture = Pin<Box<dyn Future<Output = Result<TransferGrant, StatusCode>> + Send>>;
pub type Redeemer = Arc<dyn Fn(String) -> RedeemFuture + Send + Sync>;

#[derive(Clone)]
pub struct TransferState {
    redeemer: Redeemer,
    permits: Arc<Semaphore>,
}

impl TransferState {
    pub fn new(redeemer: Redeemer) -> Self {
        Self {
            redeemer,
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_TRANSFERS)),
        }
    }
}

/// Lit l'adresse annoncée (`url_var`) et le port d'écoute (`port_var`, sinon le port de l'adresse).
pub fn transfer_settings(url_var: &str, port_var: &str) -> (Option<String>, Option<u16>) {
    let url = std::env::var(url_var)
        .ok()
        .and_then(|raw| crate::repository::valid_transfer_url(&raw));
    let port = std::env::var(port_var)
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .or_else(|| {
            url.as_deref()
                .and_then(|raw| reqwest::Url::parse(raw).ok())
                .and_then(|parsed| parsed.port())
        });
    (url, port)
}

/// Démarre le serveur de transfert en lecture seule sur `port` (toutes interfaces).
pub fn spawn_listener(port: u16, state: TransferState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let address = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => {
                tracing::info!(%address, "serveur de transfert de sources démarré");
                let _ = axum::serve(listener, transfer_router(state)).await;
            }
            Err(error) => {
                tracing::warn!(%address, error = %error, "serveur de transfert indisponible");
            }
        }
    })
}

/// Redeemer de production : interroge le central avec la clé du worker.
pub fn central_redeemer(
    client: reqwest::Client,
    central: reqwest::Url,
    worker_key: String,
    worker_id: Uuid,
) -> Redeemer {
    Arc::new(move |token| {
        let client = client.clone();
        let url = format!(
            "{}/api/v1/worker/transfers/redeem",
            central.as_str().trim_end_matches('/')
        );
        let key = worker_key.clone();
        Box::pin(async move {
            let response = client
                .post(url)
                .bearer_auth(key)
                .timeout(REDEEM_TIMEOUT)
                .json(&RedeemRequest { token, worker_id })
                .send()
                .await
                .map_err(|_| StatusCode::BAD_GATEWAY)?;
            match response.status() {
                status if status.is_success() => response
                    .json::<TransferGrant>()
                    .await
                    .map_err(|_| StatusCode::BAD_GATEWAY),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(StatusCode::FORBIDDEN),
                StatusCode::CONFLICT => Err(StatusCode::CONFLICT),
                _ => Err(StatusCode::BAD_GATEWAY),
            }
        })
    })
}

/// Routeur du serveur de transfert : une seule route, en lecture seule.
pub fn transfer_router(state: TransferState) -> Router {
    Router::new()
        .route(TRANSFER_SOURCE_PATH, get(serve_source))
        .with_state(state)
}

/// Plage d'octets invalide ou non prise en charge (réponse 416).
#[derive(Debug, PartialEq, Eq)]
pub struct UnsatisfiableRange;

/// Plage d'octets demandée, bornes incluses ; `None` pour le fichier entier.
pub fn parse_range(
    header: Option<&str>,
    total: u64,
) -> Result<Option<(u64, u64)>, UnsatisfiableRange> {
    let Some(raw) = header else {
        return Ok(None);
    };
    let spec = raw
        .trim()
        .strip_prefix("bytes=")
        .ok_or(UnsatisfiableRange)?;
    if spec.contains(',') {
        return Err(UnsatisfiableRange);
    }
    let (start, end) = spec.split_once('-').ok_or(UnsatisfiableRange)?;
    let start: u64 = start.trim().parse().map_err(|_| UnsatisfiableRange)?;
    let last = total.checked_sub(1).ok_or(UnsatisfiableRange)?;
    let end = match end.trim() {
        "" => last,
        value => value
            .parse::<u64>()
            .map_err(|_| UnsatisfiableRange)?
            .min(last),
    };
    if start > end || start > last {
        return Err(UnsatisfiableRange);
    }
    Ok(Some((start, end)))
}

async fn serve_source(State(state): State<TransferState>, headers: HeaderMap) -> Response {
    let Some(token) = headers
        .get(TRANSFER_TICKET_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let grant = match (state.redeemer)(token.to_owned()).await {
        Ok(grant) => grant,
        Err(status) => return status.into_response(),
    };
    // Le permis n'est pris qu'après validation du billet : des requêtes sans billet valide ne
    // doivent pas épuiser les transferts simultanés des vrais preneurs.
    let Ok(permit) = state.permits.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let path = match library_scan::source_path(Path::new(&grant.root), &grant.source_name).await {
        Ok(path) => path,
        Err(AppError::Invalid) => return StatusCode::FORBIDDEN.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let Ok(mut file) = tokio::fs::File::open(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(metadata) = file.metadata().await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let total = metadata.len();
    if i64::try_from(total).ok() != Some(grant.bytes) {
        return StatusCode::CONFLICT.into_response();
    }
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let (status, start, end) = match parse_range(range, total) {
        Ok(Some((start, end))) => (StatusCode::PARTIAL_CONTENT, start, end),
        Ok(None) => (StatusCode::OK, 0, total.saturating_sub(1)),
        Err(UnsatisfiableRange) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{total}"))],
            )
                .into_response();
        }
    };
    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let length = if total == 0 { 0 } else { end - start + 1 };
    let stream = futures_util::stream::unfold(
        (ReaderStream::new(file.take(length)), permit),
        |(mut reader, permit)| async move { reader.next().await.map(|chunk| (chunk, (reader, permit))) },
    );
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if status == StatusCode::PARTIAL_CONTENT
        && let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{total}"))
    {
        headers.insert(header::CONTENT_RANGE, value);
    }
    response
}

/// Écrit le corps de la réponse dans `dest`. `resume_from == 0` crée le fichier (création
/// exclusive) ; sinon les octets sont ajoutés à la fin d'un fichier partiel de cette taille.
async fn write_body(
    response: reqwest::Response,
    dest: &Path,
    resume_from: u64,
    max_bytes: u64,
) -> anyhow::Result<()> {
    let mut options = tokio::fs::OpenOptions::new();
    if resume_from == 0 {
        options.write(true).create_new(true);
    } else {
        options.append(true);
    }
    let mut file = options.open(dest).await?;
    let mut chunks = response.bytes_stream();
    let mut total = resume_from;
    let streamed = async {
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk?;
            total += chunk.len() as u64;
            anyhow::ensure!(total <= max_bytes, SOURCE_INTEGRITY);
            file.write_all(&chunk).await?;
        }
        anyhow::Ok(())
    }
    .await;
    // Même interrompu, le préfixe reçu doit être sur disque pour pouvoir reprendre.
    let flushed = file.flush().await;
    streamed?;
    flushed?;
    file.sync_all().await?;
    Ok(())
}

/// Vérifie taille et SHA-256 du fichier reçu (`source_integrity` sinon).
async fn verify_received(
    dest: &Path,
    max_bytes: u64,
    expected_sha256: &str,
    expected_bytes: i64,
) -> anyhow::Result<()> {
    let (hash, bytes) = storage::fingerprint(dest, max_bytes).await?;
    anyhow::ensure!(
        hash == expected_sha256 && bytes as i64 == expected_bytes,
        SOURCE_INTEGRITY
    );
    Ok(())
}

/// Écrit la réponse dans `dest` (création exclusive) puis vérifie taille et SHA-256.
///
/// Partagé par la voie directe et la voie du central : une source altérée ou trop grosse est
/// toujours rejetée (`source_integrity`).
pub async fn save_verified(
    response: reqwest::Response,
    dest: &Path,
    max_bytes: u64,
    expected_sha256: &str,
    expected_bytes: i64,
) -> anyhow::Result<()> {
    write_body(response, dest, 0, max_bytes).await?;
    verify_received(dest, max_bytes, expected_sha256, expected_bytes).await
}

/// Premier octet d'un en-tête `Content-Range: bytes début-fin/total`.
fn content_range_start(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get(header::CONTENT_RANGE)?.to_str().ok()?;
    let range = value.strip_prefix("bytes ")?;
    range.split_once('-')?.0.trim().parse().ok()
}

/// Taille déjà reçue d'un téléchargement interrompu (0 sans fichier).
async fn received_len(dest: &Path) -> anyhow::Result<u64> {
    match tokio::fs::metadata(dest).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

/// Lit la source directement chez le worker qui la détient (voie directe, ADR 0019).
///
/// Reprend un fichier partiel déjà présent dans `dest` par `Range` (taille et SHA-256 vérifiés sur
/// le fichier entier à la fin). Si le serveur ignore `Range`, le fichier est recommencé.
pub async fn download_direct(
    client: &reqwest::Client,
    ticket: &SourceTicket,
    dest: &Path,
    max_bytes: u64,
    expected_sha256: &str,
    expected_bytes: i64,
) -> anyhow::Result<()> {
    let expected = u64::try_from(expected_bytes)?;
    let mut received = received_len(dest).await?;
    if received > expected {
        tokio::fs::remove_file(dest).await?;
        received = 0;
    }
    if received > 0 && received == expected {
        return verify_received(dest, max_bytes, expected_sha256, expected_bytes).await;
    }
    let mut request = client
        .get(format!("{}{TRANSFER_SOURCE_PATH}", ticket.url))
        .header(TRANSFER_TICKET_HEADER, &ticket.token)
        .timeout(DIRECT_DOWNLOAD_TIMEOUT);
    if received > 0 {
        request = request.header(header::RANGE, format!("bytes={received}-"));
    }
    let response = request.send().await?.error_for_status()?;
    let offset = match (received, response.status()) {
        (0, _) => 0,
        (_, StatusCode::PARTIAL_CONTENT) => {
            anyhow::ensure!(
                content_range_start(response.headers()) == Some(received),
                "range_mismatch"
            );
            received
        }
        // Le serveur renvoie le fichier entier : on repart de zéro plutôt que de le concaténer.
        _ => {
            tokio::fs::remove_file(dest).await?;
            0
        }
    };
    write_body(response, dest, offset, max_bytes).await?;
    verify_received(dest, max_bytes, expected_sha256, expected_bytes).await
}

/// Lecture directe avec reprise : après une interruption ou un billet expiré, `reissue` demande un
/// billet frais au central et le téléchargement reprend par `Range` là où le fichier partiel
/// s'arrête. `Ok(None)` (la source se lit autrement) ou trop de reprises rendent l'erreur ; une
/// source altérée (`source_integrity`) n'est jamais reprise.
pub async fn download_direct_resuming<Reissue, Pending>(
    client: &reqwest::Client,
    mut ticket: SourceTicket,
    dest: &Path,
    max_bytes: u64,
    expected_sha256: &str,
    expected_bytes: i64,
    mut reissue: Reissue,
) -> anyhow::Result<()>
where
    Reissue: FnMut() -> Pending,
    Pending: Future<Output = anyhow::Result<Option<SourceTicket>>>,
{
    let mut resumes = 0;
    loop {
        let error = match download_direct(
            client,
            &ticket,
            dest,
            max_bytes,
            expected_sha256,
            expected_bytes,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        resumes += 1;
        if resumes > DIRECT_RESUME_ATTEMPTS || error.to_string() == SOURCE_INTEGRITY {
            return Err(error);
        }
        tracing::warn!(error = %error, resumes, "voie directe interrompue, reprise avec un billet frais");
        tokio::time::sleep(DIRECT_RESUME_DELAY).await;
        ticket = reissue().await?.ok_or(error)?;
    }
}

/// Client dédié à la voie directe : connexion courte pour basculer vite sur le repli.
pub fn direct_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(DIRECT_CONNECT_TIMEOUT)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opaque_token::sha256_hex;
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn range_parsing_accepts_simple_ranges_and_rejects_the_rest() {
        assert_eq!(parse_range(None, 100), Ok(None));
        assert_eq!(parse_range(Some("bytes=10-"), 100), Ok(Some((10, 99))));
        assert_eq!(parse_range(Some("bytes=10-19"), 100), Ok(Some((10, 19))));
        assert_eq!(parse_range(Some("bytes=90-500"), 100), Ok(Some((90, 99))));
        assert_eq!(
            parse_range(Some("bytes=100-"), 100),
            Err(UnsatisfiableRange)
        );
        assert_eq!(
            parse_range(Some("bytes=0-1,5-6"), 100),
            Err(UnsatisfiableRange)
        );
        assert_eq!(parse_range(Some("bytes=-5"), 100), Err(UnsatisfiableRange));
        assert_eq!(parse_range(Some("items=0-1"), 100), Err(UnsatisfiableRange));
        assert_eq!(parse_range(Some("bytes=0-"), 0), Err(UnsatisfiableRange));
    }

    fn state_for(dir: &Path, name: &str, bytes: i64) -> TransferState {
        let root = dir.to_string_lossy().into_owned();
        let name = name.to_owned();
        TransferState::new(Arc::new(move |token| {
            let (root, name) = (root.clone(), name.clone());
            Box::pin(async move {
                if token == "bon-billet" {
                    Ok(TransferGrant {
                        root,
                        source_name: name,
                        sha256: String::new(),
                        bytes,
                    })
                } else {
                    Err(StatusCode::FORBIDDEN)
                }
            })
        }))
    }

    async fn get_source(
        state: TransferState,
        ticket: Option<&str>,
        range: Option<&str>,
    ) -> anyhow::Result<(StatusCode, Vec<u8>)> {
        let mut request = Request::builder().uri(TRANSFER_SOURCE_PATH);
        if let Some(ticket) = ticket {
            request = request.header(TRANSFER_TICKET_HEADER, ticket);
        }
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        let response = transfer_router(state)
            .oneshot(request.body(Body::empty())?)
            .await?;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 20).await?;
        Ok((status, body.to_vec()))
    }

    #[tokio::test]
    async fn serves_the_granted_file_whole_or_by_range() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("film.mkv"), b"0123456789")?;
        let state = state_for(dir.path(), "film.mkv", 10);
        let (status, body) = get_source(state.clone(), Some("bon-billet"), None).await?;
        assert_eq!(
            (status, body.as_slice()),
            (StatusCode::OK, &b"0123456789"[..])
        );
        let (status, body) =
            get_source(state.clone(), Some("bon-billet"), Some("bytes=4-")).await?;
        assert_eq!(
            (status, body.as_slice()),
            (StatusCode::PARTIAL_CONTENT, &b"456789"[..])
        );
        let (status, _) = get_source(state, Some("bon-billet"), Some("bytes=50-")).await?;
        assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
        Ok(())
    }

    #[tokio::test]
    async fn refuses_missing_or_wrong_tickets_and_changed_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("film.mkv"), b"0123456789")?;
        let state = state_for(dir.path(), "film.mkv", 10);
        let (status, _) = get_source(state.clone(), None, None).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = get_source(state, Some("mauvais"), None).await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let changed = state_for(dir.path(), "film.mkv", 999);
        let (status, _) = get_source(changed, Some("bon-billet"), None).await?;
        assert_eq!(status, StatusCode::CONFLICT);
        Ok(())
    }

    #[tokio::test]
    async fn transfer_limit_holds_until_the_response_body_is_dropped() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("film.mkv"), b"0123456789")?;
        let state = state_for(dir.path(), "film.mkv", 10);
        let mut responses = Vec::new();
        for _ in 0..MAX_CONCURRENT_TRANSFERS {
            let request = Request::builder()
                .uri(TRANSFER_SOURCE_PATH)
                .header(TRANSFER_TICKET_HEADER, "bon-billet")
                .body(Body::empty())?;
            let response = transfer_router(state.clone()).oneshot(request).await?;
            assert_eq!(response.status(), StatusCode::OK);
            responses.push(response);
        }
        let (status, _) = get_source(state.clone(), Some("bon-billet"), None).await?;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        responses.pop();
        let (status, body) = get_source(state, Some("bon-billet"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"0123456789");
        Ok(())
    }

    #[tokio::test]
    async fn invalid_tickets_never_consume_a_transfer_permit() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("film.mkv"), b"0123456789")?;
        let state = state_for(dir.path(), "film.mkv", 10);
        let _held = state
            .permits
            .clone()
            .acquire_many_owned(u32::try_from(MAX_CONCURRENT_TRANSFERS)?)
            .await?;
        let (status, _) = get_source(state.clone(), None, None).await?;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "sans billet, jamais 503");
        let (status, _) = get_source(state.clone(), Some("mauvais"), None).await?;
        assert_eq!(status, StatusCode::FORBIDDEN, "mauvais billet, jamais 503");
        let (status, _) = get_source(state, Some("bon-billet"), None).await?;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "seul un billet valide subit la limite"
        );
        Ok(())
    }

    /// Démarre le serveur de transfert sur un port libre et rend son adresse.
    async fn spawn_server(state: TransferState) -> anyhow::Result<String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, transfer_router(state)).await;
        });
        Ok(format!("http://{address}"))
    }

    #[tokio::test]
    async fn direct_download_verifies_size_and_sha256() -> anyhow::Result<()> {
        let source_dir = tempfile::tempdir()?;
        let work_dir = tempfile::tempdir()?;
        std::fs::write(source_dir.path().join("film.mkv"), b"0123456789")?;
        let url = spawn_server(state_for(source_dir.path(), "film.mkv", 10)).await?;
        let ticket = SourceTicket {
            url,
            token: "bon-billet".into(),
        };
        let client = direct_client()?;
        let good = work_dir.path().join("ok.mkv");
        download_direct(
            &client,
            &ticket,
            &good,
            1 << 20,
            &sha256_hex(b"0123456789"),
            10,
        )
        .await?;
        assert_eq!(std::fs::read(&good)?, b"0123456789");

        let corrupted = work_dir.path().join("bad.mkv");
        let error = download_direct(
            &client,
            &ticket,
            &corrupted,
            1 << 20,
            &sha256_hex(b"autre"),
            10,
        )
        .await
        .err()
        .map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some("source_integrity"));

        let too_big = work_dir.path().join("big.mkv");
        assert!(
            download_direct(
                &client,
                &ticket,
                &too_big,
                4,
                &sha256_hex(b"0123456789"),
                10
            )
            .await
            .is_err(),
            "une source plus grosse que la limite est rejetée"
        );
        Ok(())
    }

    #[tokio::test]
    async fn direct_download_fails_fast_when_the_source_worker_is_unreachable() -> anyhow::Result<()>
    {
        let work_dir = tempfile::tempdir()?;
        let ticket = SourceTicket {
            url: "http://127.0.0.1:1".into(),
            token: "bon-billet".into(),
        };
        let dest = work_dir.path().join("x.mkv");
        assert!(
            download_direct(&direct_client()?, &ticket, &dest, 1 << 20, "", 0)
                .await
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn content_range_start_reads_the_first_byte_only_from_a_byte_range() {
        let headers = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_RANGE, HeaderValue::from_static(value));
            headers
        };
        assert_eq!(content_range_start(&headers("bytes 4-9/10")), Some(4));
        assert_eq!(content_range_start(&headers("bytes */10")), None);
        assert_eq!(content_range_start(&headers("items 4-9/10")), None);
        assert_eq!(content_range_start(&HeaderMap::new()), None);
    }

    /// Fichier partiel déjà reçu, puis téléchargement direct du fichier `0123456789`.
    async fn resume_from(partial: &[u8]) -> anyhow::Result<(anyhow::Result<()>, Vec<u8>)> {
        let source_dir = tempfile::tempdir()?;
        let work_dir = tempfile::tempdir()?;
        std::fs::write(source_dir.path().join("film.mkv"), b"0123456789")?;
        let url = spawn_server(state_for(source_dir.path(), "film.mkv", 10)).await?;
        let ticket = SourceTicket {
            url,
            token: "bon-billet".into(),
        };
        let dest = work_dir.path().join("partiel.mkv");
        std::fs::write(&dest, partial)?;
        let result = download_direct(
            &direct_client()?,
            &ticket,
            &dest,
            1 << 20,
            &sha256_hex(b"0123456789"),
            10,
        )
        .await;
        Ok((result, std::fs::read(&dest).unwrap_or_default()))
    }

    #[tokio::test]
    async fn an_interrupted_download_resumes_with_a_range_request() -> anyhow::Result<()> {
        let (result, content) = resume_from(b"0123").await?;
        result?;
        assert_eq!(content, b"0123456789");
        Ok(())
    }

    #[tokio::test]
    async fn a_partial_file_longer_than_the_source_restarts_from_zero() -> anyhow::Result<()> {
        let (result, content) = resume_from(b"0123456789-et-plus-encore").await?;
        result?;
        assert_eq!(content, b"0123456789");
        Ok(())
    }

    #[tokio::test]
    async fn a_corrupted_partial_file_is_rejected_by_the_final_hash() -> anyhow::Result<()> {
        let (result, _) = resume_from(b"XXXX").await?;
        let error = result.err().map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some("source_integrity"));
        Ok(())
    }

    #[tokio::test]
    async fn a_complete_partial_file_is_verified_without_any_request() -> anyhow::Result<()> {
        let work_dir = tempfile::tempdir()?;
        let dest = work_dir.path().join("complet.mkv");
        std::fs::write(&dest, b"0123456789")?;
        let ticket = SourceTicket {
            url: "http://127.0.0.1:1".into(),
            token: "inutile".into(),
        };
        download_direct(
            &direct_client()?,
            &ticket,
            &dest,
            1 << 20,
            &sha256_hex(b"0123456789"),
            10,
        )
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_server_that_ignores_range_makes_the_download_restart() -> anyhow::Result<()> {
        let whole_file = get(|| async { "0123456789" });
        let app = Router::new().route(TRANSFER_SOURCE_PATH, whole_file);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let work_dir = tempfile::tempdir()?;
        let dest = work_dir.path().join("partiel.mkv");
        std::fs::write(&dest, b"0123")?;
        let ticket = SourceTicket {
            url,
            token: "peu-importe".into(),
        };
        download_direct(
            &direct_client()?,
            &ticket,
            &dest,
            1 << 20,
            &sha256_hex(b"0123456789"),
            10,
        )
        .await?;
        assert_eq!(
            std::fs::read(&dest)?,
            b"0123456789",
            "jamais de concaténation d'un corps complet sur un partiel"
        );
        Ok(())
    }

    /// Reprise avec un fournisseur de billets factice qui compte ses appels.
    async fn resume_with_reissued(
        first_token: &str,
        reissued_token: Option<&str>,
        expected_sha256: &str,
    ) -> anyhow::Result<(anyhow::Result<()>, usize)> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let source_dir = tempfile::tempdir()?;
        let work_dir = tempfile::tempdir()?;
        std::fs::write(source_dir.path().join("film.mkv"), b"0123456789")?;
        let url = spawn_server(state_for(source_dir.path(), "film.mkv", 10)).await?;
        let ticket = |token: &str| SourceTicket {
            url: url.clone(),
            token: token.into(),
        };
        let first = ticket(first_token);
        let reissued = reissued_token.map(ticket);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let result = download_direct_resuming(
            &direct_client()?,
            first,
            &work_dir.path().join("reprise.mkv"),
            1 << 20,
            expected_sha256,
            10,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                let reissued = reissued.clone();
                async move { anyhow::Ok(reissued) }
            },
        )
        .await;
        Ok((result, calls.load(Ordering::SeqCst)))
    }

    #[tokio::test]
    async fn an_expired_ticket_is_replaced_by_a_fresh_one_and_the_download_completes()
    -> anyhow::Result<()> {
        let (result, reissues) =
            resume_with_reissued("perime", Some("bon-billet"), &sha256_hex(b"0123456789")).await?;
        result?;
        assert_eq!(reissues, 1);
        Ok(())
    }

    #[tokio::test]
    async fn the_download_gives_up_when_the_central_has_no_fresh_ticket() -> anyhow::Result<()> {
        let (result, reissues) =
            resume_with_reissued("perime", None, &sha256_hex(b"0123456789")).await?;
        assert!(result.is_err());
        assert_eq!(reissues, 1);
        Ok(())
    }

    #[tokio::test]
    async fn an_altered_source_is_never_resumed() -> anyhow::Result<()> {
        let (result, reissues) =
            resume_with_reissued("bon-billet", None, &sha256_hex(b"autre")).await?;
        let error = result.err().map(|error| error.to_string());
        assert_eq!(error.as_deref(), Some(SOURCE_INTEGRITY));
        assert_eq!(reissues, 0, "aucun nouveau billet pour une source altérée");
        Ok(())
    }

    #[tokio::test]
    async fn never_leaves_the_granted_root() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let state = state_for(dir.path(), "../secret", 1);
        let (status, _) = get_source(state, Some("bon-billet"), None).await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
        Ok(())
    }
}
