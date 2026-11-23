//! Séparation du moteur et des dépôts (ADR 0029) : reprise au démarrage, baux identiques avant et après.

mod common;
#[path = "common/installation.rs"]
mod installation;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use common::store::{OFFICIAL, TOOL_VERSION, TestRepository, script_runtime, tool_runtime};
use forgearr::{
    api::{AppState, router},
    domain::{JobStatus, MAX_ATTEMPTS, UpdateFlowRequest},
    plugins, repository,
};
use forgearr_shared::plugins::store::{DEV_SOURCE_ID, ItemOrigin};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

const BODY_LIMIT_BYTES: usize = 1024 * 1024;
const THIRD_PARTY: &str = "repo.tiers";
const THIRD_PARTY_URL: &str = "https://tiers.example/index.json";
/// Ce que l'installation de test utilise du dépôt officiel.
const USED: [&str; 5] = ["audio", "essentiel", "ffmpeg", "mkvtoolnix", "video"];

struct Central {
    state: AppState,
    worker: Value,
    _dir: tempfile::TempDir,
}

impl Central {
    async fn new() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        Self::with_detected(dir, None).await
    }

    async fn with_detected(
        dir: tempfile::TempDir,
        detected: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let blank = installation::blank_state(dir.path()).await?;
        common::install_keys(&blank.pool).await?;
        let mut config = (*blank.config).clone();
        config.data_dir = dir.path().join("data");
        config.tools_cache_dir = dir.path().join("tools");
        config.detected_plugins_dir = detected;
        let state = AppState::new(blank.pool, Arc::new(config));
        let mut central = Self {
            state,
            worker: Value::Null,
            _dir: dir,
        };
        let registration = json!({"name": "test-worker"});
        let (status, worker) = central
            .call("POST", "/api/v1/workers", Some(registration))
            .await?;
        anyhow::ensure!(status == StatusCode::OK, "{status}");
        central.worker = worker["id"].clone();
        Ok(central)
    }

    fn copy_of(&self, repository_id: &str) -> PathBuf {
        self.state.config.plugin_repos_dir().join(repository_id)
    }

    fn installed_dir(&self) -> PathBuf {
        self.state.config.installed_dir()
    }

    /// Clé de worker pour ses routes, d'administrateur sinon.
    async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let is_worker_route = ["/api/v1/worker", "/api/v1/claims"]
            .iter()
            .any(|prefix| uri.starts_with(prefix));
        let key = if is_worker_route {
            common::WORKER_KEY
        } else {
            common::ADMIN_KEY
        };
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {key}"))
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |json| Body::from(json.to_string())))?;
        let response = router(self.state.clone()).oneshot(request).await?;
        let status = response.status();
        let bytes = to_bytes(response.into_body(), BODY_LIMIT_BYTES).await?;
        Ok((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }

    async fn resume(&self, synced: Option<&str>) -> anyhow::Result<bool> {
        self.resume_without(synced, &[]).await
    }

    /// Comme après une synchronisation où le serveur a refusé les fichiers `withheld`.
    async fn resume_without(
        &self,
        synced: Option<&str>,
        withheld: &[String],
    ) -> anyhow::Result<bool> {
        let state = &self.state;
        let synced = synced.map(|id| plugins::SyncedRepository { id, withheld });
        Ok(plugins::resume_store(&state.pool, &state.config, &state.catalog_cache, synced).await?)
    }

    async fn settings_belong_to(&self, plugin: &str, origin: &ItemOrigin) -> anyhow::Result<bool> {
        let mut conn = self.state.pool.acquire().await?;
        Ok(repository::settings_belong_to(&mut conn, plugin, origin).await?)
    }

    async fn queue(&self, source: &str, flow: &str) -> anyhow::Result<uuid::Uuid> {
        let (pool, hash) = (&self.state.pool, "ab".repeat(32));
        let job =
            repository::insert_job_with_options(pool, source, None, &hash, 1, None, Some(flow))
                .await?;
        Ok(job.id)
    }

    async fn reserve(&self) -> anyhow::Result<(StatusCode, Value)> {
        let request = json!({"worker_id": self.worker});
        self.call("POST", "/api/v1/claims", Some(request)).await
    }

    async fn add_third_party(&self) -> anyhow::Result<()> {
        let (pool, changes) = (&self.state.pool, &self.state.changes);
        repository::add_plugin_repository(pool, changes, THIRD_PARTY, "Tiers", THIRD_PARTY_URL)
            .await?;
        Ok(())
    }

    /// Flux d'un seul nœud, de même identifiant que son type.
    async fn save_flow(&self, kind: &str) -> anyhow::Result<()> {
        let definition = json!({
            "nodes": [{"id": "n", "type": kind, "name": "N", "config": {}}], "connections": []
        });
        let flow = UpdateFlowRequest {
            name: kind.to_owned(),
            description: None,
            file_mask: "*".to_owned(),
            is_default: false,
            definition: definition.to_string(),
        };
        repository::save_flow(&self.state.pool, kind, &flow).await?;
        Ok(())
    }

    /// Crée une tâche de ce flux et la demande ; rend la réponse, et l'état de la tâche juste après.
    async fn claim(&self, flow: &str) -> anyhow::Result<(StatusCode, Value, String)> {
        let pool = &self.state.pool;
        let job = self.queue("film.mkv", flow).await?;
        let (status, claim) = self.reserve().await?;
        let after: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?1")
            .bind(job)
            .fetch_one(pool)
            .await?;
        sqlx::query("DELETE FROM jobs").execute(pool).await?;
        Ok((status, claim, after))
    }

    /// Ce que le bail d'une tâche de ce flux tient du moteur.
    async fn contracts(&self, flow: &str) -> anyhow::Result<Value> {
        let (status, claim, _) = self.claim(flow).await?;
        anyhow::ensure!(status == StatusCode::OK, "{flow} : {status}");
        Ok(json!([
            claim["operations"],
            claim["block_operations"],
            claim["required_tools"],
            claim["probe_tool_id"],
            claim["probe_version"],
        ]))
    }

    /// Éléments inscrits, triés.
    async fn installed(&self) -> anyhow::Result<Vec<String>> {
        let mut ids: Vec<String> = repository::list_installations(&self.state.pool)
            .await?
            .into_iter()
            .map(|item| item.record.item_id)
            .collect();
        ids.sort();
        Ok(ids)
    }

    async fn record(&self, id: &str) -> anyhow::Result<repository::InstallationRecord> {
        let mut conn = self.state.pool.acquire().await?;
        repository::get_installation(&mut conn, id)
            .await?
            .map(|item| item.record)
            .ok_or_else(|| anyhow::anyhow!("{id} n'est pas installé"))
    }

    async fn frozen_root(&self, id: &str) -> anyhow::Result<PathBuf> {
        let record = self.record(id).await?;
        plugins::installed_root(&self.installed_dir(), &record)
            .ok_or_else(|| anyhow::anyhow!("dossier figé de {id}"))
    }

    async fn pending(&self) -> anyhow::Result<Option<Vec<String>>> {
        Ok(repository::pending_store_repositories(&self.state.pool).await?)
    }

    async fn worker_tools_status(&self) -> anyhow::Result<StatusCode> {
        Ok(self.call("GET", "/api/v1/worker/tools", None).await?.0)
    }

    async fn official_origin(&self) -> anyhow::Result<ItemOrigin> {
        let repo = repository::get_plugin_repository(&self.state.pool, OFFICIAL).await?;
        Ok(ItemOrigin {
            repository_id: repo.id,
            repository_url: repo.index_url,
            repository_name: repo.name,
        })
    }
}

fn operation<'a>(contracts: &'a Value, id: &str) -> &'a Value {
    &contracts[0][id]
}

#[tokio::test]
async fn an_offline_resumption_changes_no_lease_and_leaves_the_rest_to_the_store()
-> anyhow::Result<()> {
    let central = Central::new().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    for kind in ["audio-run", "route.run", "essentiel.run"] {
        central.save_flow(kind).await?;
    }
    let (pool, changes) = (&central.state.pool, &central.state.changes);
    repository::set_plugin_settings(pool, changes, "notif", r#"{"url":"http://x"}"#).await?;
    let before = [
        central.contracts("audio-run").await?,
        central.contracts("route.run").await?,
    ];
    assert_eq!(
        operation(&before[1], "operation.script.route")["runtime"]["code"],
        "print('route')"
    );
    assert_eq!(central.pending().await?, None);

    assert!(central.resume(None).await?);
    assert_eq!(central.pending().await?, Some(Vec::new()));
    let mut wanted: Vec<&str> = USED.to_vec();
    wanted.extend(["notif", "curl", "route"]);
    wanted.sort_unstable();
    assert_eq!(central.installed().await?, wanted);

    // Les mêmes baux, sans la copie du dépôt.
    std::fs::remove_dir_all(central.copy_of(OFFICIAL))?;
    central.state.catalog_cache.invalidate().await;
    let after = [
        central.contracts("audio-run").await?,
        central.contracts("route.run").await?,
    ];
    assert_eq!(after, before);
    let tools = &after[0][2];
    assert!(
        tools
            .as_array()
            .is_some_and(|tools| tools.len() == 2
                && tools.iter().all(|tool| tool["version"] == TOOL_VERSION)),
        "{tools}"
    );

    // Origine des éléments et de leurs réglages.
    let origin = central.official_origin().await?;
    assert_eq!(central.record("audio").await?.origin, origin);
    let script = central.record("route").await?;
    assert_eq!(script.copy_id.as_deref(), Some("route"));
    assert!(!script.copy_pending);
    assert!(central.settings_belong_to("notif", &origin).await?);
    // Réglages saisis une fois le plugin installé.
    let (status, _) = central
        .call(
            "PUT",
            "/api/v1/plugins/audio/settings",
            Some(json!({"preset": "fort"})),
        )
        .await?;
    assert_eq!(status, StatusCode::OK);
    assert!(central.settings_belong_to("audio", &origin).await?);

    // Le catalogue du moteur et les outils à garder ne viennent plus que de l'installé.
    let (_, listed) = central.call("GET", "/api/v1/plugins", None).await?;
    assert!(!listed.to_string().contains("inutile"), "{listed}");
    let (status, tools) = central.call("GET", "/api/v1/worker/tools", None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tools.as_array().map(Vec::len), Some(3));
    Ok(())
}

#[tokio::test]
async fn neither_a_second_resumption_nor_a_new_publication_changes_the_engine() -> anyhow::Result<()>
{
    let central = Central::new().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    central.save_flow("audio.run").await?;
    assert!(central.resume(None).await?);
    let before = central.contracts("audio.run").await?;
    let rows = repository::list_installations(&central.state.pool).await?;

    // Nouvelle publication : opération changée, plugin ajouté, outil retiré.
    std::fs::remove_dir_all(central.copy_of(OFFICIAL))?;
    TestRepository::new(central.copy_of(OFFICIAL))
        .plugin("audio", &tool_runtime("autre"))?
        .plugin("nouveau", &tool_runtime("autre"))?
        .publish()?;
    central.state.catalog_cache.invalidate().await;
    assert!(central.resume(Some(OFFICIAL)).await?);
    assert_eq!(
        repository::list_installations(&central.state.pool).await?,
        rows
    );
    assert_eq!(central.contracts("audio.run").await?, before);

    // Un dépôt ajouté ensuite n'est qu'une vitrine.
    central.add_third_party().await?;
    TestRepository::new(central.copy_of(THIRD_PARTY))
        .plugin("tiers", &tool_runtime("autre"))?
        .publish()?;
    let (_, listed) = central.call("GET", "/api/v1/plugins", None).await?;
    assert!(!listed.to_string().contains("tiers"), "{listed}");
    let sources = plugins::store_sources(&central.state.pool, &central.state.config).await?;
    let ids: Vec<&str> = sources
        .iter()
        .map(|source| source.origin.repository_id.as_str())
        .collect();
    assert_eq!(ids, [OFFICIAL, THIRD_PARTY]);
    Ok(())
}

#[tokio::test]
async fn a_repository_without_a_copy_is_awaited_then_can_be_removed() -> anyhow::Result<()> {
    let central = Central::new().await?;
    central.add_third_party().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    central.save_flow("audio.run").await?;
    central.save_flow("tiers.run").await?;
    assert!(!central.resume(None).await?);
    assert_eq!(central.pending().await?, Some(vec![THIRD_PARTY.to_owned()]));
    assert_eq!(central.installed().await?, USED);
    assert_eq!(
        central.worker_tools_status().await?,
        StatusCode::BAD_GATEWAY
    );
    // Le dépôt officiel, repris, n'est déjà plus lu.
    std::fs::remove_dir_all(central.copy_of(OFFICIAL))?;
    central.state.catalog_cache.invalidate().await;
    assert!(central.contracts("audio.run").await.is_ok());

    TestRepository::new(central.copy_of(THIRD_PARTY))
        .plugin("tiers", &tool_runtime("ffmpeg"))?
        .publish()?;
    central.state.catalog_cache.invalidate().await;
    assert!(central.resume(Some(THIRD_PARTY)).await?);
    assert_eq!(central.worker_tools_status().await?, StatusCode::OK);
    let before = central.contracts("tiers.run").await?;
    assert!(operation(&before, "operation.tiers.run").is_object());

    let uri = format!("/api/v1/plugins/repositories/{THIRD_PARTY}");
    let (status, _) = central.call("DELETE", &uri, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(!central.copy_of(THIRD_PARTY).exists());
    assert_eq!(central.contracts("tiers.run").await?, before);
    Ok(())
}

#[tokio::test]
async fn a_defective_item_stops_neither_the_others_nor_its_flows() -> anyhow::Result<()> {
    let central = Central::new().await?;
    central.add_third_party().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    let third_party = || {
        TestRepository::new(central.copy_of(THIRD_PARTY))
            .plugin("malade", &tool_runtime("ffmpeg"))?
            .plugin("sain", &tool_runtime("ffmpeg"))
    };
    let wrong = json!({"plugins/malade/operations/run.json": "0".repeat(64)});
    third_party()?.publish_with_digests(&wrong)?;
    for kind in ["malade.run", "sain.run"] {
        central.save_flow(kind).await?;
    }
    let before = central.contracts("malade.run").await?;
    assert!(operation(&before, "operation.malade.run").is_object());

    assert!(!central.resume(Some(THIRD_PARTY)).await?);
    assert_eq!(central.pending().await?, Some(vec![THIRD_PARTY.to_owned()]));
    let installed = central.installed().await?;
    assert!(installed.contains(&"sain".to_owned()));
    assert!(!installed.contains(&"malade".to_owned()));
    assert_eq!(central.contracts("malade.run").await?, before);

    // Retirer ce dépôt casserait un flux ; le désactiver ne change rien au moteur.
    let uri = format!("/api/v1/plugins/repositories/{THIRD_PARTY}");
    let (status, refused) = central.call("DELETE", &uri, None).await?;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refused["error"]["code"], "store_migrating");
    let toggle = format!("{uri}/toggle");
    let (status, _) = central.call("PUT", &toggle, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(central.contracts("malade.run").await?, before);

    third_party()?.publish()?;
    let (status, _) = central.call("DELETE", &uri, None).await?;
    assert_eq!(status, StatusCode::OK, "repris avant le retrait");
    assert_eq!(central.contracts("malade.run").await?, before);
    Ok(())
}

#[tokio::test]
async fn a_file_is_missing_for_good_only_after_a_synchronisation() -> anyhow::Result<()> {
    let central = Central::new().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?
        .plugin("muet", &script_runtime())?
        .publish()?;
    central.save_flow("muet.run").await?;
    assert!(!central.resume(None).await?);
    assert!(!central.installed().await?.contains(&"muet".to_owned()));
    let before = central.contracts("muet.run").await?;

    assert!(central.resume(Some(OFFICIAL)).await?);
    let frozen = plugins::read_item(&central.frozen_root("muet").await?, "muet")?;
    assert_eq!(frozen.missing, ["plugins/muet/run.py"]);
    assert_eq!(central.contracts("muet.run").await?, before);
    Ok(())
}

#[tokio::test]
async fn an_interruption_is_resumed_where_it_stopped() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let copy = TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    for kind in ["audio.run", "route.run"] {
        central.save_flow(kind).await?;
    }
    let before = central.contracts("route.run").await?;

    // Arrêt entre le gel et l'inscription d'« audio », et avant la copie modifiable de « route ».
    let pool = &central.state.pool;
    repository::set_pending_store_repositories(pool, &[OFFICIAL.to_owned()]).await?;
    let origin = central.official_origin().await?;
    let lock = plugins::lock_installations().await;
    for id in ["audio", "route"] {
        let content = plugins::read_item(&copy, id)?;
        let record = repository::InstallationRecord {
            item_id: id.to_owned(),
            kind: content.entry.entry_type,
            version: content.entry.version.clone(),
            content_sha256: content.content_sha256.clone(),
            origin: origin.clone(),
            copy_id: Some(id.to_owned()),
            copy_pending: true,
            serves_every_task: false,
        };
        plugins::freeze_item(&lock, &central.installed_dir(), content, &origin).await?;
        if id == "route" {
            let mut conn = pool.acquire().await?;
            repository::save_installation(&mut conn, &record).await?;
        }
    }
    drop(lock);
    // Le dépôt, encore lu, supplée ce qui manque.
    assert_eq!(central.contracts("route.run").await?, before);

    assert!(central.resume(None).await?);
    let mut wanted: Vec<&str> = USED.to_vec();
    wanted.insert(wanted.len() - 1, "route");
    assert_eq!(central.installed().await?, wanted);
    assert!(!central.record("route").await?.copy_pending);
    assert_eq!(central.contracts("route.run").await?, before);

    // Arrêt juste avant la fin : tout est déjà inscrit.
    let rows = repository::list_installations(pool).await?;
    repository::set_pending_store_repositories(pool, &[OFFICIAL.to_owned()]).await?;
    assert!(central.resume(None).await?);
    assert_eq!(repository::list_installations(pool).await?, rows);

    // Une copie figée effacée pendant la reprise est refaite depuis le dépôt.
    std::fs::remove_dir_all(central.frozen_root("audio").await?)?;
    repository::set_pending_store_repositories(pool, &[OFFICIAL.to_owned()]).await?;
    assert!(central.resume(None).await?);
    assert!(central.frozen_root("audio").await?.is_dir());

    // Même s'il publie autre chose depuis : le moteur le lit déjà. Un autre dépôt, jamais.
    std::fs::remove_dir_all(central.frozen_root("audio").await?)?;
    TestRepository::official(central.copy_of(OFFICIAL))?
        .plugin("audio", &tool_runtime("curl"))?
        .publish()?;
    central.state.catalog_cache.invalidate().await;
    repository::set_pending_store_repositories(pool, &[OFFICIAL.to_owned()]).await?;
    let elsewhere = "https://ailleurs.example/index.json";
    for (url, replaced) in [(elsewhere, false), (origin.repository_url.as_str(), true)] {
        sqlx::query("UPDATE installed_items SET repository_url = ?1 WHERE item_id = 'audio'")
            .bind(url)
            .execute(pool)
            .await?;
        assert_eq!(central.resume(None).await?, replaced);
        assert_eq!(central.frozen_root("audio").await?.is_dir(), replaced);
    }
    Ok(())
}

#[tokio::test]
async fn the_items_of_a_repository_come_from_the_same_publication() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let publish = |tool: &str| {
        TestRepository::official(central.copy_of(OFFICIAL))?
            .plugin("lecteur", &tool_runtime(tool))?
            .plugin("muet", &script_runtime())?
            .publish()
    };
    publish("ffmpeg")?;
    for kind in ["lecteur.run", "muet.run"] {
        central.save_flow(kind).await?;
    }
    // Une synchronisation peut encore apporter le code de « muet » : rien n'est figé avant.
    assert!(!central.resume(None).await?);
    assert!(central.installed().await?.is_empty());

    let copy = publish("curl")?;
    central.state.catalog_cache.invalidate().await;
    let withheld = ["plugins/muet/run.py".to_owned()];
    assert!(!central.resume_without(Some(OFFICIAL), &withheld).await?);
    let installed = central.installed().await?;
    assert!(installed.contains(&"lecteur".to_owned()));
    assert!(
        !installed.contains(&"muet".to_owned()),
        "un refus du serveur n'est pas une absence"
    );
    let contracts = central.contracts("lecteur.run").await?;
    let operation = operation(&contracts, "operation.lecteur.run");
    assert_eq!(operation["runtime"]["tool"]["id"], "curl");

    std::fs::write(copy.join("plugins/muet/run.py"), "print(1)")?;
    assert!(central.resume(Some(OFFICIAL)).await?);
    let frozen = plugins::read_item(&central.frozen_root("muet").await?, "muet")?;
    assert!(frozen.missing.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_configured_plugin_waits_for_the_repository_that_outranks_its_namesake()
-> anyhow::Result<()> {
    let central = Central::new().await?;
    central.add_third_party().await?;
    TestRepository::new(central.copy_of(THIRD_PARTY))
        .plugin("notif", &tool_runtime("curl"))?
        .publish()?;
    let (pool, changes) = (&central.state.pool, &central.state.changes);
    repository::set_plugin_settings(pool, changes, "notif", r#"{"url":"http://x"}"#).await?;
    assert!(!central.resume(Some(THIRD_PARTY)).await?);
    assert!(central.installed().await?.is_empty());

    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    central.state.catalog_cache.invalidate().await;
    assert!(central.resume(Some(OFFICIAL)).await?);
    let origin = central.official_origin().await?;
    assert_eq!(central.record("notif").await?.origin, origin);
    assert!(central.settings_belong_to("notif", &origin).await?);
    Ok(())
}

#[tokio::test]
async fn a_repository_added_again_elsewhere_is_not_the_one_to_resume() -> anyhow::Result<()> {
    let central = Central::new().await?;
    central.add_third_party().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    central.save_flow("tiers.run").await?;
    assert!(!central.resume(None).await?);
    let uri = format!("/api/v1/plugins/repositories/{THIRD_PARTY}");
    let (status, _) = central.call("DELETE", &uri, None).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(central.pending().await?, Some(Vec::new()));

    let state = &central.state;
    let elsewhere = "https://ailleurs.example/index.json";
    plugins::add_repository(
        &state.pool,
        &state.catalog_cache,
        &state.config,
        THIRD_PARTY,
        "Tiers",
        elsewhere,
    )
    .await?;
    TestRepository::new(central.copy_of(THIRD_PARTY))
        .plugin("tiers", &tool_runtime("ffmpeg"))?
        .publish()?;
    state.catalog_cache.invalidate().await;
    assert!(central.resume(Some(THIRD_PARTY)).await?);
    assert!(!central.installed().await?.contains(&"tiers".to_owned()));
    let (_, listed) = central.call("GET", "/api/v1/plugins", None).await?;
    assert!(!listed.to_string().contains("tiers"), "{listed}");
    Ok(())
}

#[tokio::test]
async fn a_script_that_cannot_be_copied_is_never_recorded() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let copy = TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    central.save_flow("route.run").await?;
    let manifest = copy.join("scripts/route/manifest.json");
    let text = std::fs::read_to_string(&manifest)?;
    std::fs::write(
        &manifest,
        text.replace(r#""id":"route""#, r#""id":"autre""#),
    )?;
    assert!(!central.resume(Some(OFFICIAL)).await?);
    assert!(!central.installed().await?.contains(&"route".to_owned()));
    let engine = plugins::engine_sources(&central.state.pool, &central.state.config).await?;
    assert!(engine.damaged.is_empty());
    assert!(central.installed().await?.contains(&"video".to_owned()));

    // Corrigé par son dépôt, il est repris.
    std::fs::write(&manifest, text)?;
    assert!(central.resume(Some(OFFICIAL)).await?);
    assert!(!central.record("route").await?.copy_pending);
    Ok(())
}

#[tokio::test]
async fn a_new_installation_receives_the_base_after_its_first_synchronisation() -> anyhow::Result<()>
{
    let central = Central::new().await?;
    assert!(!central.resume(None).await?);
    assert_eq!(central.pending().await?, Some(vec![OFFICIAL.to_owned()]));
    assert!(central.installed().await?.is_empty());

    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    central.state.catalog_cache.invalidate().await;
    assert!(central.resume(Some(OFFICIAL)).await?);
    assert_eq!(
        central.installed().await?,
        ["essentiel", "mkvtoolnix", "video"]
    );
    Ok(())
}

#[tokio::test]
async fn a_task_whose_frozen_copy_is_missing_waits_for_its_repair() -> anyhow::Result<()> {
    let central = Central::new().await?;
    TestRepository::official(central.copy_of(OFFICIAL))?.publish()?;
    for kind in ["audio.run", "essentiel.run"] {
        central.save_flow(kind).await?;
    }
    assert!(central.resume(None).await?);
    let root = central.frozen_root("audio").await?;
    let aside = root.with_extension("ailleurs");
    std::fs::rename(&root, &aside)?;

    // Plus de demandes qu'il n'en faudrait pour épuiser ses tentatives.
    let held = central.queue("retenu.mkv", "audio.run").await?;
    for _ in 0..i64::from(MAX_ATTEMPTS) + repository::MAX_FREE_RELEASES + 1 {
        assert_eq!(central.reserve().await?, (StatusCode::OK, Value::Null));
    }
    // Une tâche plus récente qui n'en dépend pas s'exécute ; aucun outil n'est purgé d'ici là.
    let healthy = central.queue("sain.mkv", "essentiel.run").await?;
    let (status, claim) = central.reserve().await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(claim["job_id"], json!(healthy));
    let waiting = repository::get_job(&central.state.pool, held).await?;
    assert_eq!(waiting.status, JobStatus::Queued);
    assert_eq!(waiting.attempts, 0);
    sqlx::query("DELETE FROM jobs")
        .execute(&central.state.pool)
        .await?;
    assert_eq!(
        central.worker_tools_status().await?,
        StatusCode::BAD_GATEWAY
    );

    std::fs::rename(&aside, &root)?;
    let (status, _, _) = central.claim("audio.run").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(central.worker_tools_status().await?, StatusCode::OK);

    // Le porteur de la sonde sert à toute tâche.
    assert!(!central.record("audio").await?.serves_every_task);
    assert!(central.record("video").await?.serves_every_task);
    std::fs::remove_dir_all(central.frozen_root("video").await?)?;
    let (status, claim, after) = central.claim("essentiel.run").await?;
    assert_eq!((status, after.as_str()), (StatusCode::OK, "queued"));
    assert!(claim.is_null());
    Ok(())
}

#[tokio::test]
async fn a_detected_plugin_folder_is_resumed_then_only_a_source_of_the_store() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let detected = dir.path().join("voisin");
    TestRepository::official(detected.clone())?.publish()?;
    let central = Central::with_detected(dir, Some(detected.clone())).await?;
    central.save_flow("audio.run").await?;
    let before = central.contracts("audio.run").await?;
    // Tout ce qui sert est trouvé : le dépôt officiel, sans copie, n'est pas attendu.
    assert!(central.resume(None).await?);
    assert_eq!(central.installed().await?, USED);
    let origin = central.record("audio").await?.origin;
    assert_eq!(origin.repository_id, DEV_SOURCE_ID);

    std::fs::write(
        detected.join("plugins/audio/operations/run.json"),
        "modifiée",
    )?;
    central.state.catalog_cache.invalidate().await;
    assert_eq!(central.contracts("audio.run").await?, before);
    Ok(())
}
