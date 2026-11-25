//! Homonymes, noms déjà pris, réglages et place dans le moteur.

use super::{
    OFFICIAL, THIRD_PARTY, actions, ensure_plain,
    fixture::{Central, target},
    item,
    item::{Item, base, binary, primitive, reading_settings_of},
};
use forgearr_shared::plugins::store::{
    InstallState,
    plan::{
        ClaimKind,
        PlanAction::{AlreadyInstalled, Install},
        PlanBlocker, PlanConsent, PlanWarning,
    },
};
use serde_json::json;

fn video() -> Item {
    Item::plugin("video", base())
}

#[tokio::test]
async fn a_single_homonym_is_installable() -> anyhow::Result<()> {
    let central = Central::with_installed(&[video()]).await?;
    let flow = Item::flow("flux", &["video.run"])
        .depending(json!([{"id": "video", "versionRange": "^1.0.0"}]));
    central.publish(THIRD_PARTY, &[video(), flow]).await?;
    let origin = central.origin(OFFICIAL).await?;
    let state = central
        .context()
        .await?
        .install_state(&target(THIRD_PARTY, "video"))?;
    assert_eq!(
        state,
        InstallState::HeldByOther {
            origin: origin.clone()
        }
    );
    let prepared = central.plan(&[(THIRD_PARTY, "video")]).await?;
    assert_eq!(
        prepared.plan.blockers,
        [PlanBlocker::HeldByOther {
            id: "video".to_owned(),
            origin: origin.clone(),
        }]
    );
    // Comme dépendance, celui qui est installé convient, et le plan le dit.
    let prepared = central.plan(&[(THIRD_PARTY, "flux")]).await?;
    assert_eq!(
        actions(&prepared),
        [("flux", Install), ("video", AlreadyInstalled)]
    );
    assert!(prepared.plan.blockers.is_empty());
    assert_eq!(
        prepared.plan.warnings,
        [PlanWarning::OtherOrigin {
            id: "video".to_owned(),
            origin,
        }]
    );
    Ok(())
}

#[tokio::test]
async fn two_homonyms_of_the_same_plan_exclude_each_other() -> anyhow::Result<()> {
    let central = Central::new().await?;
    central.publish(OFFICIAL, &[video()]).await?;
    central.publish(THIRD_PARTY, &[video()]).await?;
    let prepared = central
        .plan(&[(OFFICIAL, "video"), (THIRD_PARTY, "video")])
        .await?;
    assert!(matches!(
        prepared.plan.blockers.as_slice(),
        [PlanBlocker::HeldByOther { origin, .. }] if origin.repository_id == OFFICIAL
    ));
    assert_eq!(prepared.changes.len(), 1);
    Ok(())
}

#[tokio::test]
async fn operations_kinds_and_aliases_are_unique() -> anyhow::Result<()> {
    let central = Central::with_installed(&[video()]).await?;
    let thief = Item::plugin("voleur", primitive())
        .operation("operation.video.run", primitive())
        .block("voleur.bis", "video-run", "operation.voleur.run");
    // Nom sous lequel le moteur cherche le script `route`.
    let script = Item::script("route", primitive());
    let hijacker = Item::new("plugin", "pirate").operation("operation.route", primitive());
    central
        .publish(THIRD_PARTY, &[thief, script, hijacker])
        .await?;
    let prepared = central.plan(&[(THIRD_PARTY, "voleur")]).await?;
    let conflict = |id: &str, claim, value: &str, holder: &str| PlanBlocker::ClaimConflict {
        id: id.to_owned(),
        claim,
        value: value.to_owned(),
        holder: holder.to_owned(),
    };
    assert_eq!(
        prepared.plan.blockers,
        [
            conflict("voleur", ClaimKind::BlockKind, "video-run", "video"),
            conflict(
                "voleur",
                ClaimKind::Operation,
                "operation.video.run",
                "video"
            ),
        ]
    );
    // Entre deux éléments du même plan.
    let prepared = central
        .plan(&[(THIRD_PARTY, "route"), (THIRD_PARTY, "pirate")])
        .await?;
    assert_eq!(
        prepared.plan.blockers,
        [conflict(
            "pirate",
            ClaimKind::Operation,
            "operation.route",
            "route"
        )]
    );
    Ok(())
}

#[tokio::test]
async fn an_inherited_conflict_does_not_prevent_an_update() -> anyhow::Result<()> {
    let twin = |version: &str| {
        Item::plugin("jumeau", primitive())
            .operation("operation.video.run", primitive())
            .version(version)
    };
    let central = Central::with_installed(&[video(), twin("1.0.0")]).await?;
    central.publish(OFFICIAL, &[video(), twin("1.1.0")]).await?;
    ensure_plain(&central.plan(&[(OFFICIAL, "jumeau")]).await?)?;

    let worse = twin("1.1.0").block("video-run", "jumeau-bis", "operation.jumeau.run");
    central.publish(OFFICIAL, &[video(), worse]).await?;
    let prepared = central.plan(&[(OFFICIAL, "jumeau")]).await?;
    assert!(matches!(
        prepared.plan.blockers.as_slice(),
        [PlanBlocker::ClaimConflict { value, .. }] if value == "video-run"
    ));
    Ok(())
}

#[tokio::test]
async fn an_operation_speaks_for_its_own_plugin_only() -> anyhow::Result<()> {
    let video = video().exposing("mediainfo", "1.0.0", &["codec"]);
    let central = Central::with_installed(&[video]).await?;
    let usurper = Item::plugin("tiers", primitive()).declaring("providerId", json!("video"));
    let sharing = Item::plugin("partage", primitive()).exposing("mediainfo", "1.0.0", &["codec"]);
    central.publish(THIRD_PARTY, &[usurper, sharing]).await?;
    let prepared = central.plan(&[(THIRD_PARTY, "tiers")]).await?;
    assert_eq!(
        prepared.plan.blockers,
        [PlanBlocker::ForeignProvider {
            id: "tiers".to_owned(),
            operation: "operation.tiers.run".to_owned(),
            provider: "video".to_owned(),
        }]
    );
    // Un espace partagé reste permis.
    let prepared = central.plan(&[(THIRD_PARTY, "partage")]).await?;
    assert!(prepared.plan.blockers.is_empty());
    assert_eq!(
        prepared.plan.warnings,
        [PlanWarning::SharedNamespace {
            id: "partage".to_owned(),
            namespace: "mediainfo".to_owned(),
            holder: "video".to_owned(),
        }]
    );
    Ok(())
}

#[tokio::test]
async fn settings_are_discarded_or_read_by_another_plugin_with_consent() -> anyhow::Result<()> {
    let sonarr = || Item::plugin("sonarr", reading_settings_of("sonarr"));
    let reader = Item::plugin("lecteur", reading_settings_of("sonarr"));
    let central = Central::new().await?;
    central.publish(OFFICIAL, &[sonarr()]).await?;
    central.publish(THIRD_PARTY, &[sonarr(), reader]).await?;
    central.save_settings("sonarr", OFFICIAL).await?;
    ensure_plain(&central.plan(&[(OFFICIAL, "sonarr")]).await?)?;
    let discard = PlanConsent::DiscardSettings {
        id: "sonarr".to_owned(),
    };
    let prepared = central.plan(&[(THIRD_PARTY, "sonarr")]).await?;
    assert_eq!(prepared.plan.consents, std::slice::from_ref(&discard));
    assert!(prepared.plan.blockers.is_empty());

    // Le plugin dont les réglages sont lus est requis, pas seulement cité.
    let reads = PlanConsent::ReadsSettings {
        id: "lecteur".to_owned(),
        plugin: "sonarr".to_owned(),
    };
    let prepared = central.plan(&[(THIRD_PARTY, "lecteur")]).await?;
    assert_eq!(
        actions(&prepared),
        [("lecteur", Install), ("sonarr", Install)]
    );
    assert_eq!(prepared.plan.consents, [reads.clone(), discard]);

    central.install(OFFICIAL, "sonarr").await?;
    let prepared = central.plan(&[(THIRD_PARTY, "lecteur")]).await?;
    assert_eq!(prepared.plan.consents, [reads]);
    assert_eq!(item(&prepared, "sonarr")?.action, AlreadyInstalled);
    Ok(())
}

#[tokio::test]
async fn taking_the_place_of_the_probe_or_of_the_default_operation_needs_consent()
-> anyhow::Result<()> {
    let central = Central::with_installed(&[video()]).await?;
    // Sonde d'une seule extension : elle passe devant celle qui les prend toutes.
    let probe = Item::plugin(
        "sonde",
        json!({
            "kind": "binary", "executable": "x", "executionMode": "probe-primary",
            "probeMatch": {"extensions": [".mkv"], "priority": -5}
        }),
    );
    let mut runtime = binary("x");
    runtime["defaultFallback"] = json!(true);
    let default = Item::new("plugin", "defaut").operation("operation.a", runtime.clone());
    // Choisie après celle en place : elle ne remplace rien.
    let spare = Item::new("plugin", "secours").operation("operation.z", runtime);
    let enrich = Item::plugin(
        "appoint",
        json!({"kind": "binary", "executable": "x", "executionMode": "probe-enrich"}),
    );
    central
        .publish(THIRD_PARTY, &[probe, default, spare, enrich])
        .await?;
    let consents = async |id: &str| -> anyhow::Result<Vec<PlanConsent>> {
        Ok(central.plan(&[(THIRD_PARTY, id)]).await?.plan.consents)
    };
    assert_eq!(
        consents("sonde").await?,
        [PlanConsent::ReplacesPrimaryProbe {
            id: "sonde".to_owned(),
            current: "video".to_owned(),
        }]
    );
    assert_eq!(
        consents("defaut").await?,
        [PlanConsent::ReplacesDefaultOperation {
            id: "defaut".to_owned(),
            current: "video".to_owned(),
        }]
    );
    assert!(consents("secours").await?.is_empty());
    assert!(consents("appoint").await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_incomplete_item_is_refused_unless_only_operations_are_left_out() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let without_block = video().omitting("blocks/0.json");
    let partial = Item::plugin("partiel", primitive())
        .operation("operation.partiel.bis", primitive())
        .omitting("operations/1.json");
    let empty = Item::plugin("vide", primitive()).omitting("operations/0.json");
    central
        .publish(OFFICIAL, &[without_block, partial, empty])
        .await?;
    let files = |id: &str, path: &str| (id.to_owned(), vec![format!("plugins/{id}/{path}")]);
    for (id, path) in [("video", "blocks/0.json"), ("vide", "operations/0.json")] {
        let prepared = central.plan(&[(OFFICIAL, id)]).await?;
        let (id, files) = files(id, path);
        assert_eq!(
            prepared.plan.blockers,
            [PlanBlocker::Incomplete { id, files }]
        );
    }
    let prepared = central.plan(&[(OFFICIAL, "partiel")]).await?;
    let (id, files) = files("partiel", "operations/1.json");
    assert!(prepared.plan.blockers.is_empty());
    assert_eq!(
        prepared.plan.consents,
        [PlanConsent::PartialInstall { id, files }]
    );
    let state = central
        .context()
        .await?
        .install_state(&target(OFFICIAL, "video"))?;
    assert!(matches!(state, InstallState::Incomplete { .. }));
    Ok(())
}
