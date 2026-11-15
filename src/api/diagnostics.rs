//! Diagnostic réseau : contrôles purs sur la configuration, l'origine du navigateur,
//! les en-têtes de la requête et la présence des workers. Réservé aux administrateurs.

use super::{AdminAuth, AppState};
use crate::{error::AppResult, repository};
use axum::{Json, extract::State, http::HeaderMap};
use chrono::{DateTime, Utc};
use flux_shared::diagnostics::{
    CheckId, CheckStatus, DiagnosticCheck, DiagnosticsReport, DiagnosticsRequest,
};
use flux_shared::workers::{Liveness, Worker};

const FORWARDED_PROTO: &str = "x-forwarded-proto";
const FORWARDED_HOST: &str = "x-forwarded-host";
const FORWARDED_FOR: &str = "x-forwarded-for";
const FORWARDED: &str = "forwarded";
const HTTPS_SCHEME: &str = "https";
const LOCAL_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// Ce que la requête de diagnostic révèle d'un éventuel proxy inverse.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyHints {
    pub forwarded_proto: Option<String>,
    pub forwarded_host: Option<String>,
    /// Un en-tête de proxy quelconque est présent.
    pub proxied: bool,
}

impl ProxyHints {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let read = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let forwarded_proto = read(FORWARDED_PROTO);
        let forwarded_host = read(FORWARDED_HOST);
        let proxied = forwarded_proto.is_some()
            || forwarded_host.is_some()
            || read(FORWARDED_FOR).is_some()
            || read(FORWARDED).is_some();
        Self {
            forwarded_proto,
            forwarded_host,
            proxied,
        }
    }
}

/// Données d'entrée des contrôles, toutes déjà lues.
pub struct DiagnosticInput<'a> {
    pub web_origin: Option<&'a str>,
    pub browser_origin: &'a str,
    pub tls_direct: bool,
    pub smtp_configured: bool,
    pub proxy: &'a ProxyHints,
    pub workers: &'a [Worker],
    pub now: DateTime<Utc>,
}

fn normalize(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn host_of(origin: &str) -> &str {
    let rest = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    rest.split(['/', ':']).next().unwrap_or(rest)
}

fn is_https(origin: &str) -> bool {
    normalize(origin).starts_with("https://")
}

fn is_local(origin: &str) -> bool {
    let host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    let host = host.split('/').next().unwrap_or(host);
    LOCAL_HOSTS
        .iter()
        .any(|local| host == *local || host.starts_with(&format!("{local}:")))
}

fn check(
    id: CheckId,
    status: CheckStatus,
    explanation: impl Into<String>,
    fix: &str,
) -> DiagnosticCheck {
    DiagnosticCheck {
        id,
        status,
        explanation: explanation.into(),
        fix: (status != CheckStatus::Ok).then(|| fix.to_owned()),
    }
}

pub fn check_public_origin(input: &DiagnosticInput) -> DiagnosticCheck {
    const FIX: &str = "Réglez WEB_ORIGIN sur l'adresse exacte saisie dans le navigateur (schéma, hôte et port), puis redémarrez le conteneur.";
    match input.web_origin {
        None => check(
            CheckId::PublicOrigin,
            CheckStatus::Warning,
            "WEB_ORIGIN n'est pas défini : les sessions suivent l'adresse d'accès, mais les liens envoyés par courriel n'ont pas d'adresse publique.",
            FIX,
        ),
        Some(origin) if normalize(origin) == normalize(input.browser_origin) => check(
            CheckId::PublicOrigin,
            CheckStatus::Ok,
            format!("WEB_ORIGIN ({origin}) correspond à l'adresse utilisée dans ce navigateur."),
            FIX,
        ),
        Some(origin) => check(
            CheckId::PublicOrigin,
            CheckStatus::Error,
            format!(
                "WEB_ORIGIN vaut {origin} mais ce navigateur est sur {} : les connexions et actions de l'interface seront refusées.",
                input.browser_origin
            ),
            FIX,
        ),
    }
}

pub fn check_tls(input: &DiagnosticInput) -> DiagnosticCheck {
    const FIX: &str = "Servez Flux en HTTPS : placez un reverse proxy TLS devant le port 3000, ou fournissez TLS_CERT et TLS_KEY au central.";
    let proxy_https = input
        .proxy
        .forwarded_proto
        .as_deref()
        .is_some_and(|proto| proto.eq_ignore_ascii_case(HTTPS_SCHEME));
    if input.tls_direct {
        check(
            CheckId::Tls,
            CheckStatus::Ok,
            "Le central sert lui-même le TLS.",
            FIX,
        )
    } else if proxy_https || is_https(input.browser_origin) {
        check(
            CheckId::Tls,
            CheckStatus::Ok,
            "Le TLS est terminé par un proxy externe ; le central reçoit du HTTP simple.",
            FIX,
        )
    } else if is_local(input.browser_origin) {
        check(
            CheckId::Tls,
            CheckStatus::Ok,
            "HTTP simple sur une adresse locale : acceptable pour un essai.",
            FIX,
        )
    } else {
        check(
            CheckId::Tls,
            CheckStatus::Warning,
            "L'interface est servie en HTTP simple : identifiants, cookies et clés transitent en clair.",
            FIX,
        )
    }
}

pub fn check_proxy_headers(input: &DiagnosticInput) -> DiagnosticCheck {
    const FIX: &str = "Faites transmettre X-Forwarded-Proto, X-Forwarded-Host et X-Forwarded-For par le proxy (nginx : proxy_set_header ; Traefik et Caddy le font par défaut).";
    let behind_https_proxy = !input.tls_direct && is_https(input.browser_origin);
    if input.proxy.proxied {
        let host_matches = input
            .proxy
            .forwarded_host
            .as_deref()
            .is_none_or(|host| host_of(host) == host_of(input.browser_origin));
        if host_matches {
            check(
                CheckId::ProxyHeaders,
                CheckStatus::Ok,
                "Un proxy est détecté et transmet ses en-têtes X-Forwarded-*.",
                FIX,
            )
        } else {
            check(
                CheckId::ProxyHeaders,
                CheckStatus::Warning,
                "X-Forwarded-Host ne correspond pas à l'hôte du navigateur.",
                FIX,
            )
        }
    } else if behind_https_proxy {
        check(
            CheckId::ProxyHeaders,
            CheckStatus::Warning,
            "Le navigateur est en HTTPS mais la requête n'arrive avec aucun en-tête X-Forwarded-* : le proxy ne les transmet pas.",
            FIX,
        )
    } else {
        check(
            CheckId::ProxyHeaders,
            CheckStatus::Ok,
            "Aucun proxy détecté : accès direct au central.",
            FIX,
        )
    }
}

/// Le flux SSE ne peut pas être éprouvé depuis une requête ordinaire : derrière un proxy,
/// on rappelle la réglage qui le casse (mise en tampon des réponses).
pub fn check_event_stream(input: &DiagnosticInput) -> DiagnosticCheck {
    const FIX: &str = "Désactivez la mise en tampon pour /api/v1/jobs/events (nginx : proxy_buffering off ; proxy_read_timeout 1h) et gardez la connexion HTTP/1.1 ouverte.";
    if input.proxy.proxied {
        check(
            CheckId::EventStream,
            CheckStatus::Warning,
            "Un proxy est présent : s'il met les réponses en tampon, la progression des tâches n'apparaît qu'à la fin. Ce point ne peut pas être éprouvé depuis le central.",
            FIX,
        )
    } else {
        check(
            CheckId::EventStream,
            CheckStatus::Ok,
            "Pas de proxy entre le navigateur et le central : le flux de progression n'est pas filtré.",
            FIX,
        )
    }
}

pub fn check_workers(input: &DiagnosticInput) -> DiagnosticCheck {
    const FIX: &str = "Vérifiez que le worker tourne, qu'il joint CENTRAL_URL et que sa clé d'API n'est pas révoquée (journaux du conteneur worker).";
    let remote: Vec<&Worker> = input
        .workers
        .iter()
        .filter(|worker| !worker.is_integrated())
        .collect();
    if remote.is_empty() {
        return check(
            CheckId::Workers,
            CheckStatus::Ok,
            "Aucun worker distant raccordé.",
            FIX,
        );
    }
    let count_of = |liveness: Liveness| {
        remote
            .iter()
            .filter(|worker| worker.liveness(input.now) == liveness)
            .count()
    };
    let (silent, offline) = (count_of(Liveness::Silent), count_of(Liveness::Offline));
    if silent == remote.len() {
        check(
            CheckId::Workers,
            CheckStatus::Error,
            format!(
                "Aucun des {} workers distants n'a contacté le central récemment.",
                remote.len()
            ),
            FIX,
        )
    } else if silent + offline > 0 {
        check(
            CheckId::Workers,
            CheckStatus::Warning,
            format!(
                "{} worker(s) distant(s) sur {} ne répondent plus.",
                silent + offline,
                remote.len()
            ),
            FIX,
        )
    } else {
        check(
            CheckId::Workers,
            CheckStatus::Ok,
            format!("Les {} workers distants sont joignables.", remote.len()),
            FIX,
        )
    }
}

pub fn check_smtp(input: &DiagnosticInput) -> DiagnosticCheck {
    const FIX: &str = "Définissez SMTP_HOST, SMTP_FROM (et les identifiants si besoin) ainsi que WEB_ORIGIN pour activer « mot de passe oublié ».";
    match (input.smtp_configured, input.web_origin.is_some()) {
        (true, true) => check(
            CheckId::Smtp,
            CheckStatus::Ok,
            "SMTP est configuré : le mot de passe oublié envoie un lien par courriel.",
            FIX,
        ),
        (true, false) => check(
            CheckId::Smtp,
            CheckStatus::Warning,
            "SMTP est configuré mais WEB_ORIGIN manque : aucun lien ne peut être construit.",
            FIX,
        ),
        (false, _) => check(
            CheckId::Smtp,
            CheckStatus::Warning,
            "SMTP n'est pas configuré : le mot de passe oublié est désactivé.",
            FIX,
        ),
    }
}

pub fn run_checks(input: &DiagnosticInput) -> DiagnosticsReport {
    DiagnosticsReport {
        checks: vec![
            check_public_origin(input),
            check_tls(input),
            check_proxy_headers(input),
            check_event_stream(input),
            check_workers(input),
            check_smtp(input),
        ],
    }
}

pub async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    _auth: AdminAuth,
    Json(request): Json<DiagnosticsRequest>,
) -> AppResult<Json<DiagnosticsReport>> {
    let workers = repository::list_workers(&state.pool).await?;
    let proxy = ProxyHints::from_headers(&headers);
    let effective = crate::runtime_settings::load_effective(&state.pool, &state.config).await;
    let config = &effective.config;
    Ok(Json(run_checks(&DiagnosticInput {
        web_origin: config.web_origin.as_deref(),
        browser_origin: &request.browser_origin,
        tls_direct: config.tls.is_some(),
        smtp_configured: config.smtp.is_some(),
        proxy: &proxy,
        workers: &workers,
        now: Utc::now(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use chrono::Duration;
    use flux_shared::workers::{INTEGRATED_WORKER_NAME, WORKER_SILENT_ALERT_SECONDS};
    use uuid::Uuid;

    const ORIGIN: &str = "https://flux.example";

    fn worker(name: &str, seconds_ago: i64, now: DateTime<Utc>) -> Worker {
        Worker {
            id: Uuid::nil(),
            name: name.into(),
            last_seen: now - Duration::seconds(seconds_ago),
            version: None,
            remote_addr: None,
            telemetry: None,
        }
    }

    fn input<'a>(proxy: &'a ProxyHints, workers: &'a [Worker]) -> DiagnosticInput<'a> {
        DiagnosticInput {
            web_origin: Some(ORIGIN),
            browser_origin: ORIGIN,
            tls_direct: false,
            smtp_configured: true,
            proxy,
            workers,
            now: Utc::now(),
        }
    }

    fn proxy_https() -> ProxyHints {
        ProxyHints {
            forwarded_proto: Some("https".into()),
            forwarded_host: Some("flux.example".into()),
            proxied: true,
        }
    }

    #[test]
    fn proxy_hints_read_forwarded_headers() {
        let mut headers = HeaderMap::new();
        assert!(!ProxyHints::from_headers(&headers).proxied);
        headers.insert(FORWARDED_PROTO, HeaderValue::from_static("https"));
        let hints = ProxyHints::from_headers(&headers);
        assert!(hints.proxied);
        assert_eq!(hints.forwarded_proto.as_deref(), Some("https"));
    }

    #[test]
    fn origin_mismatch_is_red_and_match_is_green_ignoring_case_and_slash() {
        let proxy = ProxyHints::default();
        let mut data = input(&proxy, &[]);
        assert_eq!(check_public_origin(&data).status, CheckStatus::Ok);
        assert!(check_public_origin(&data).fix.is_none());
        data.web_origin = Some("HTTPS://Flux.example/");
        assert_eq!(check_public_origin(&data).status, CheckStatus::Ok);
        data.web_origin = Some("https://autre.example");
        let result = check_public_origin(&data);
        assert_eq!(result.status, CheckStatus::Error);
        assert!(result.fix.is_some());
        data.web_origin = None;
        assert_eq!(check_public_origin(&data).status, CheckStatus::Warning);
    }

    #[test]
    fn tls_accepts_direct_proxy_and_local_http_but_warns_on_public_http() {
        let proxy = ProxyHints::default();
        let mut data = input(&proxy, &[]);
        data.tls_direct = true;
        data.browser_origin = "http://nas.lan:3000";
        assert_eq!(check_tls(&data).status, CheckStatus::Ok);
        data.tls_direct = false;
        assert_eq!(check_tls(&data).status, CheckStatus::Warning);
        data.browser_origin = "http://localhost:3000";
        assert_eq!(check_tls(&data).status, CheckStatus::Ok);
        data.browser_origin = ORIGIN;
        assert_eq!(check_tls(&data).status, CheckStatus::Ok);
    }

    #[test]
    fn https_without_forwarded_headers_means_the_proxy_is_incomplete() {
        let none = ProxyHints::default();
        assert_eq!(
            check_proxy_headers(&input(&none, &[])).status,
            CheckStatus::Warning
        );
        let proxy = proxy_https();
        assert_eq!(
            check_proxy_headers(&input(&proxy, &[])).status,
            CheckStatus::Ok
        );
        let wrong_host = ProxyHints {
            forwarded_host: Some("interne:3000".into()),
            ..proxy_https()
        };
        assert_eq!(
            check_proxy_headers(&input(&wrong_host, &[])).status,
            CheckStatus::Warning
        );
        let mut direct = input(&none, &[]);
        direct.browser_origin = "http://localhost:3000";
        assert_eq!(check_proxy_headers(&direct).status, CheckStatus::Ok);
    }

    #[test]
    fn event_stream_reminds_buffering_only_behind_a_proxy() {
        let none = ProxyHints::default();
        assert_eq!(
            check_event_stream(&input(&none, &[])).status,
            CheckStatus::Ok
        );
        let proxy = proxy_https();
        let result = check_event_stream(&input(&proxy, &[]));
        assert_eq!(result.status, CheckStatus::Warning);
        assert!(
            result
                .fix
                .is_some_and(|fix| fix.contains("proxy_buffering"))
        );
    }

    #[test]
    fn workers_are_graded_by_remote_silence() {
        let proxy = ProxyHints::default();
        let now = Utc::now();
        let grade = |workers: Vec<Worker>| {
            let mut data = input(&proxy, &workers);
            data.now = now;
            check_workers(&data).status
        };
        assert_eq!(grade(vec![]), CheckStatus::Ok);
        assert_eq!(
            grade(vec![worker(
                INTEGRATED_WORKER_NAME,
                WORKER_SILENT_ALERT_SECONDS * 2,
                now
            )]),
            CheckStatus::Ok
        );
        assert_eq!(grade(vec![worker("nas", 5, now)]), CheckStatus::Ok);
        assert_eq!(
            grade(vec![
                worker("nas", 5, now),
                worker("pi", WORKER_SILENT_ALERT_SECONDS, now)
            ]),
            CheckStatus::Warning
        );
        assert_eq!(
            grade(vec![worker("pi", WORKER_SILENT_ALERT_SECONDS, now)]),
            CheckStatus::Error
        );
    }

    #[test]
    fn smtp_needs_a_host_and_a_public_origin() {
        let proxy = ProxyHints::default();
        let mut data = input(&proxy, &[]);
        assert_eq!(check_smtp(&data).status, CheckStatus::Ok);
        data.web_origin = None;
        assert_eq!(check_smtp(&data).status, CheckStatus::Warning);
        data.smtp_configured = false;
        assert_eq!(check_smtp(&data).status, CheckStatus::Warning);
    }

    #[test]
    fn a_healthy_setup_is_all_green_and_lists_every_check_once() {
        let proxy = proxy_https();
        let mut data = input(&proxy, &[]);
        data.tls_direct = false;
        let report = run_checks(&data);
        assert_eq!(report.checks.len(), 6);
        let ok: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Ok)
            .collect();
        assert_eq!(ok.len(), 5, "le SSE derrière proxy reste orange");
    }
}
