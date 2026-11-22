//! Configuration réseau du central (lecture seule).

use super::system::{DetailGrid, DetailRow, duration_label};
use crate::ui::SectionHeader;
use forgearr_shared::jobs::{
    MAX_UNREACHABLE_TIMEOUT_SECONDS, MIN_UNREACHABLE_TIMEOUT_SECONDS, valid_unreachable_timeout,
};
use forgearr_shared::system::{NetworkStatus, SystemStatus};
use leptos::prelude::*;

fn tls_label(direct: bool) -> &'static str {
    if direct {
        "Servi par le central"
    } else {
        "Non (HTTP simple, proxy TLS externe conseillé)"
    }
}

fn network_rows(network: &NetworkStatus) -> Vec<DetailRow> {
    vec![
        DetailRow::mono("Adresse d'écoute", network.bind.clone()),
        DetailRow::text("TLS", tls_label(network.tls)),
        DetailRow::mono(
            "Origine web",
            network
                .web_origin
                .clone()
                .unwrap_or_else(|| "Celle de l'adresse d'accès".to_owned()),
        ),
        DetailRow::text(
            "Durée d'une session",
            duration_label(network.session_ttl_seconds),
        ),
    ]
}

/// Bloc d'environnement du central, à copier dans `compose.yaml` ou un fichier `.env` ; sans secret.
fn environment_block(network: &NetworkStatus) -> String {
    let origin = network.web_origin.as_deref().map_or_else(
        || "# WEB_ORIGIN non défini".to_owned(),
        |origin| format!("WEB_ORIGIN={origin}"),
    );
    let tls = if network.tls {
        "TLS_CERT=/chemin/certificat.pem\nTLS_KEY=/chemin/cle.pem"
    } else {
        "# TLS_CERT / TLS_KEY non définis (TLS par un proxy externe)"
    };
    format!(
        "BIND_ADDR={}\n{origin}\n{tls}\nWEB_SESSION_TTL_SECONDS={}\n",
        network.bind, network.session_ttl_seconds
    )
}

const SETTINGS_PATH: &str = "/api/v1/system/settings";
const MIN_SESSION_TTL_SECONDS: i64 = 60;
const MAX_SESSION_TTL_SECONDS: i64 = 2_592_000;

fn tls_mode_value(tls: forgearr_shared::system::SmtpTls) -> &'static str {
    match tls {
        forgearr_shared::system::SmtpTls::None => "none",
        forgearr_shared::system::SmtpTls::StartTls => "starttls",
        forgearr_shared::system::SmtpTls::Tls => "tls",
    }
}

fn parse_tls_mode(value: &str) -> forgearr_shared::system::SmtpTls {
    match value {
        "starttls" => forgearr_shared::system::SmtpTls::StartTls,
        "tls" => forgearr_shared::system::SmtpTls::Tls,
        _ => forgearr_shared::system::SmtpTls::None,
    }
}

fn trimmed_optional(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[derive(Default)]
struct SettingsFormInput<'a> {
    web_origin: &'a str,
    session_ttl: &'a str,
    /// Vide : la valeur du central est conservée.
    unreachable_timeout: &'a str,
    smtp_enabled: bool,
    smtp_host: &'a str,
    smtp_port: &'a str,
    smtp_tls: &'a str,
    smtp_username: &'a str,
    smtp_password: &'a str,
    clear_smtp_password: bool,
    smtp_from: &'a str,
    allow_private_plugin_hosts: bool,
    block_private_integration_hosts: bool,
    check_updates_enabled: bool,
}

fn parse_unreachable_timeout(raw: &str) -> Result<Option<i64>, String> {
    let Some(raw) = trimmed_optional(raw) else {
        return Ok(None);
    };
    raw.parse::<i64>()
        .ok()
        .filter(|seconds| valid_unreachable_timeout(*seconds))
        .map(Some)
        .ok_or_else(|| {
            format!(
                "Le délai d'injoignabilité doit être compris entre {MIN_UNREACHABLE_TIMEOUT_SECONDS} et {MAX_UNREACHABLE_TIMEOUT_SECONDS} secondes."
            )
        })
}

fn build_settings_request(
    input: &SettingsFormInput<'_>,
) -> Result<forgearr_shared::system::UpdateSystemSettingsRequest, String> {
    let ttl: i64 = input
        .session_ttl
        .trim()
        .parse()
        .map_err(|_| "La durée de session doit être un nombre entier (secondes).".to_owned())?;
    if !(MIN_SESSION_TTL_SECONDS..=MAX_SESSION_TTL_SECONDS).contains(&ttl) {
        return Err(format!(
            "La durée de session doit être comprise entre {MIN_SESSION_TTL_SECONDS} et {MAX_SESSION_TTL_SECONDS} secondes."
        ));
    }
    let unreachable_timeout_seconds = parse_unreachable_timeout(input.unreachable_timeout)?;
    let parsed_port = match trimmed_optional(input.smtp_port) {
        Some(raw) => {
            let port: u16 = raw
                .parse()
                .map_err(|_| "Le port SMTP doit être compris entre 1 et 65535.".to_owned())?;
            if port == 0 {
                return Err("Le port SMTP doit être supérieur à 0.".to_owned());
            }
            Some(port)
        }
        None => None,
    };
    let password = if input.clear_smtp_password {
        Some(String::new())
    } else if input.smtp_password.is_empty() {
        None
    } else {
        Some(input.smtp_password.to_owned())
    };
    Ok(forgearr_shared::system::UpdateSystemSettingsRequest {
        web_origin: trimmed_optional(input.web_origin),
        session_ttl_seconds: ttl,
        smtp_enabled: input.smtp_enabled,
        smtp_host: trimmed_optional(input.smtp_host),
        smtp_port: parsed_port,
        smtp_tls: parse_tls_mode(input.smtp_tls),
        smtp_username: trimmed_optional(input.smtp_username),
        smtp_password: password,
        smtp_from: trimmed_optional(input.smtp_from),
        allow_private_plugin_hosts: input.allow_private_plugin_hosts,
        block_private_integration_hosts: input.block_private_integration_hosts,
        check_updates_enabled: input.check_updates_enabled,
        timezone: None,
        unreachable_timeout_seconds,
    })
}

#[component]
fn SettingsRuntimeForm(initial: SystemStatus, on_saved: Callback<()>) -> impl IntoView {
    let web_origin = RwSignal::new(initial.network.web_origin.clone().unwrap_or_default());
    let session_ttl = RwSignal::new(initial.network.session_ttl_seconds.to_string());
    let unreachable_timeout = RwSignal::new(initial.unreachable_timeout_seconds.to_string());
    let smtp_enabled = RwSignal::new(initial.smtp.configured);
    let smtp_host = RwSignal::new(initial.smtp.host.clone().unwrap_or_default());
    let smtp_port = RwSignal::new(initial.smtp.port.to_string());
    let smtp_tls = RwSignal::new(tls_mode_value(initial.smtp.tls).to_owned());
    let smtp_username = RwSignal::new(initial.smtp.username.clone().unwrap_or_default());
    let smtp_from = RwSignal::new(initial.smtp.from.clone().unwrap_or_default());
    let smtp_password = RwSignal::new(String::new());
    let clear_smtp_password = RwSignal::new(false);
    let password_set = initial.smtp.password_set;
    let allow_private_plugin_hosts = RwSignal::new(initial.outbound.allow_private_plugin_hosts);
    let block_private_integration_hosts =
        RwSignal::new(initial.outbound.block_private_integration_hosts);
    let check_updates_enabled = RwSignal::new(initial.check_updates_enabled);
    let feedback = RwSignal::new(None::<String>);

    let save = Action::new_local(
        move |req: &forgearr_shared::system::UpdateSystemSettingsRequest| {
            let body = req.clone();
            async move { crate::api::put::<_, SystemStatus>(SETTINGS_PATH, &body).await }
        },
    );

    Effect::new(move |_| match save.value().get() {
        Some(Ok(_updated)) => {
            smtp_password.set(String::new());
            clear_smtp_password.set(false);
            feedback.set(None);
            crate::ui::Toasts::notify_success("Réglages système enregistrés.");
            on_saved.run(());
        }
        Some(Err(err)) => {
            crate::ui::Toasts::set_error(feedback, err.message());
        }
        None => {}
    });

    view! {
        <form class="stack" on:submit=move |event| {
            event.prevent_default();
            match build_settings_request(&SettingsFormInput {
                web_origin: &web_origin.get_untracked(),
                session_ttl: &session_ttl.get_untracked(),
                unreachable_timeout: &unreachable_timeout.get_untracked(),
                smtp_enabled: smtp_enabled.get_untracked(),
                smtp_host: &smtp_host.get_untracked(),
                smtp_port: &smtp_port.get_untracked(),
                smtp_tls: &smtp_tls.get_untracked(),
                smtp_username: &smtp_username.get_untracked(),
                smtp_password: &smtp_password.get_untracked(),
                clear_smtp_password: clear_smtp_password.get_untracked(),
                smtp_from: &smtp_from.get_untracked(),
                allow_private_plugin_hosts: allow_private_plugin_hosts.get_untracked(),
                block_private_integration_hosts: block_private_integration_hosts.get_untracked(),
                check_updates_enabled: check_updates_enabled.get_untracked(),
            }) {
                Ok(req) => {
                    feedback.set(None);
                    save.dispatch(req);
                }
                Err(msg) => feedback.set(Some(msg)),
            }
        }>
            <div class="settings-form-grid">
                <div class="form-group">
                    <label for="cfg-web-origin" class="form-label">"Origine web publique (WEB_ORIGIN)"</label>
                    <input id="cfg-web-origin" class="form-input text-mono" placeholder="https://forgearr.example"
                        prop:value=move || web_origin.get()
                        on:input=move |ev| web_origin.set(event_target_value(&ev)) />
                </div>
                <div class="form-group">
                    <label for="cfg-session-ttl" class="form-label">"Durée de session web (secondes)"</label>
                    <input id="cfg-session-ttl" class="form-input text-mono" type="number" min=MIN_SESSION_TTL_SECONDS max=MAX_SESSION_TTL_SECONDS
                        prop:value=move || session_ttl.get()
                        on:input=move |ev| session_ttl.set(event_target_value(&ev)) />
                </div>
                <div class="form-group">
                    <label for="cfg-unreachable-timeout" class="form-label">"Délai d'injoignabilité d'un worker (secondes)"</label>
                    <input id="cfg-unreachable-timeout" class="form-input text-mono" type="number"
                        min=MIN_UNREACHABLE_TIMEOUT_SECONDS max=MAX_UNREACHABLE_TIMEOUT_SECONDS
                        aria-describedby="cfg-unreachable-timeout-help"
                        prop:value=move || unreachable_timeout.get()
                        on:input=move |ev| unreachable_timeout.set(event_target_value(&ev)) />
                    <p id="cfg-unreachable-timeout-help" class="form-help">"Sans nouvelle du worker pendant ce délai, sa tâche est remise en file."</p>
                </div>
            </div>

            <fieldset class="settings-fieldset">
                <legend class="settings-fieldset__legend">"Messagerie SMTP (réinitialisation de mot de passe)"</legend>
                <label class="flow-settings__toggle">
                    <input type="checkbox"
                        prop:checked=move || smtp_enabled.get()
                        on:change=move |ev| smtp_enabled.set(event_target_checked(&ev)) />
                    <span>"Activer l'envoi d'e-mails SMTP"</span>
                </label>
                <div class="settings-form-grid">
                    <div class="form-group">
                        <label for="cfg-smtp-host" class="form-label">"Hôte SMTP"</label>
                        <input id="cfg-smtp-host" class="form-input text-mono" placeholder="smtp.exemple"
                            prop:value=move || smtp_host.get()
                            on:input=move |ev| smtp_host.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label for="cfg-smtp-port" class="form-label">"Port"</label>
                        <input id="cfg-smtp-port" class="form-input text-mono" type="number" min="1" max="65535"
                            prop:value=move || smtp_port.get()
                            on:input=move |ev| smtp_port.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label for="cfg-smtp-tls" class="form-label">"Chiffrement TLS"</label>
                        <select id="cfg-smtp-tls" class="form-input"
                            prop:value=move || smtp_tls.get()
                            on:change=move |ev| smtp_tls.set(event_target_value(&ev))>
                            <option value="starttls">"STARTTLS"</option>
                            <option value="tls">"TLS implicite (465)"</option>
                            <option value="none">"Aucun (local uniquement)"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label for="cfg-smtp-from" class="form-label">"Adresse d'expédition"</label>
                        <input id="cfg-smtp-from" class="form-input" placeholder="Forgearr <noreply@forgearr.example>"
                            prop:value=move || smtp_from.get()
                            on:input=move |ev| smtp_from.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label for="cfg-smtp-user" class="form-label">"Utilisateur SMTP"</label>
                        <input id="cfg-smtp-user" class="form-input" autocomplete="off"
                            prop:value=move || smtp_username.get()
                            on:input=move |ev| smtp_username.set(event_target_value(&ev)) />
                    </div>
                    <div class="form-group">
                        <label for="cfg-smtp-pass" class="form-label">"Mot de passe SMTP"</label>
                        <input id="cfg-smtp-pass" class="form-input" type="password" autocomplete="new-password"
                            placeholder=if password_set { "•••• (conservé si vide)" } else { "Optionnel" }
                            disabled=move || clear_smtp_password.get()
                            prop:value=move || smtp_password.get()
                            on:input=move |ev| smtp_password.set(event_target_value(&ev)) />
                    </div>
                </div>
                {password_set.then(|| view! {
                    <label class="flow-settings__toggle">
                        <input type="checkbox"
                            prop:checked=move || clear_smtp_password.get()
                            on:change=move |ev| clear_smtp_password.set(event_target_checked(&ev)) />
                        <span>"Effacer le mot de passe SMTP enregistré"</span>
                    </label>
                })}
            </fieldset>

            <fieldset class="settings-fieldset">
                <legend class="settings-fieldset__legend">"Politique des requêtes sortantes"</legend>
                <label class="flow-settings__toggle">
                    <input type="checkbox"
                        prop:checked=move || allow_private_plugin_hosts.get()
                        on:change=move |ev| allow_private_plugin_hosts.set(event_target_checked(&ev)) />
                    <span>"Autoriser les dépôts de plugins sur des adresses privées / locales"</span>
                </label>
                <label class="flow-settings__toggle">
                    <input type="checkbox"
                        prop:checked=move || block_private_integration_hosts.get()
                        on:change=move |ev| block_private_integration_hosts.set(event_target_checked(&ev)) />
                    <span>"Bloquer les adresses privées pour les intégrations (webhooks, services externes)"</span>
                </label>
                <label class="flow-settings__toggle">
                    <input type="checkbox"
                        prop:checked=move || check_updates_enabled.get()
                        on:change=move |ev| check_updates_enabled.set(event_target_checked(&ev)) />
                    <span>"Vérifier automatiquement les mises à jour publiées sur GitHub (facultatif)"</span>
                </label>
            </fieldset>

            {move || feedback.get().map(|err| view! {
                <p class="form-error" role="status">{err}</p>
            })}
            <div class="cluster-end">
                <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>
                    "Enregistrer les réglages"
                </button>
            </div>
        </form>
    }
}

#[component]
pub fn ConfigurationCard(
    status: Signal<Option<Result<SystemStatus, String>>>,
    on_saved: Callback<()>,
) -> impl IntoView {
    view! {
        <section class="section-panel settings-section">
            <SectionHeader title="Configuration réseau & runtime" subtitle="Port HTTP, origine web, sessions, SMTP et politique de sortie." />
            {move || match status.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(message)) => view! { <p class="text-danger">{message}</p> }.into_any(),
                Some(Ok(status)) => view! {
                    <div class="stack">
                        <DetailGrid rows=network_rows(&status.network) />
                        <SettingsRuntimeForm initial=status.clone() on_saved=on_saved />
                        <details>
                            <summary class="form-help">"Variables d'environnement détectées"</summary>
                            <pre class="code-block-wrapper"><code>{environment_block(&status.network)}</code></pre>
                        </details>
                    </div>
                }.into_any(),
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(tls: bool, web_origin: Option<&str>) -> NetworkStatus {
        NetworkStatus {
            bind: "0.0.0.0:3000".into(),
            tls,
            web_origin: web_origin.map(str::to_owned),
            session_ttl_seconds: 28_800,
        }
    }

    #[test]
    fn the_unreachable_timeout_is_bounded_and_optional() {
        assert_eq!(parse_unreachable_timeout(" "), Ok(None));
        assert_eq!(parse_unreachable_timeout(" 600 "), Ok(Some(600)));
        for bad in ["59", "86401", "dix"] {
            assert!(parse_unreachable_timeout(bad).is_err());
        }
    }

    #[test]
    fn tls_is_described_either_way() {
        assert!(tls_label(true).contains("central"));
        assert!(tls_label(false).contains("HTTP simple"));
    }

    #[test]
    fn rows_cover_the_four_settings() {
        assert_eq!(network_rows(&network(false, None)).len(), 4);
    }

    #[test]
    fn environment_block_mirrors_the_configuration_without_secrets() {
        let block = environment_block(&network(true, Some("https://flux.example")));
        assert!(block.contains("BIND_ADDR=0.0.0.0:3000"));
        assert!(block.contains("WEB_ORIGIN=https://flux.example"));
        assert!(block.contains("TLS_CERT="));
        assert!(block.contains("WEB_SESSION_TTL_SECONDS=28800"));
        let plain = environment_block(&network(false, None));
        assert!(plain.contains("# WEB_ORIGIN non défini"));
        assert!(!plain.contains("TLS_CERT=/"));
    }

    #[test]
    fn build_settings_request_validates_bounds_and_password_handling() -> Result<(), String> {
        assert!(
            build_settings_request(&SettingsFormInput {
                session_ttl: "10",
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            build_settings_request(&SettingsFormInput {
                session_ttl: "3600",
                smtp_enabled: true,
                smtp_host: "smtp.ex",
                smtp_port: "0",
                smtp_tls: "starttls",
                ..Default::default()
            })
            .is_err()
        );

        let kept = build_settings_request(&SettingsFormInput {
            web_origin: " https://flux.ex ",
            session_ttl: "7200",
            smtp_enabled: true,
            smtp_host: " smtp.ex ",
            smtp_port: "587",
            smtp_tls: "starttls",
            smtp_username: " user ",
            smtp_from: " Flux <a@flux.ex> ",
            allow_private_plugin_hosts: true,
            ..Default::default()
        })?;
        assert_eq!(kept.web_origin.as_deref(), Some("https://flux.ex"));
        assert_eq!(kept.session_ttl_seconds, 7200);
        assert_eq!(kept.smtp_port, Some(587));
        assert_eq!(kept.smtp_tls, forgearr_shared::system::SmtpTls::StartTls);
        assert_eq!(kept.smtp_password, None);
        assert!(kept.allow_private_plugin_hosts);
        assert!(!kept.block_private_integration_hosts);

        assert_eq!(kept.unreachable_timeout_seconds, None);

        let cleared = build_settings_request(&SettingsFormInput {
            session_ttl: "3600",
            smtp_tls: "tls",
            smtp_password: "ignored",
            clear_smtp_password: true,
            block_private_integration_hosts: true,
            ..Default::default()
        })?;
        assert_eq!(cleared.smtp_password, Some(String::new()));
        assert_eq!(cleared.smtp_tls, forgearr_shared::system::SmtpTls::Tls);
        assert!(cleared.block_private_integration_hosts);
        Ok(())
    }
}
