//! Mises à jour, réparations, et ce qu'elles retirent à ce qui s'en sert.

use super::{
    OFFICIAL, THIRD_PARTY, actions, change, ensure_plain,
    fixture::{Central, target},
    item,
    item::{Item, base, binary, primitive, script_requiring},
};
use crate::{plugins::local_catalog_dir, repository};
use forgearr_shared::plugins::store::{
    AvailableUpdate, InstallState,
    plan::{
        PlanAction::{AlreadyInstalled, Install, Repair, Update},
        PlanBlocker, PlanConsent,
    },
};
use serde_json::json;

fn video(version: &str) -> Item {
    Item::plugin("video", base()).version(version)
}

#[tokio::test]
async fn an_update_is_newer_or_another_content_from_the_same_origin() -> anyhow::Result<()> {
    let central = Central::with_installed(&[video("1.1.0")]).await?;
    let state = async || -> anyhow::Result<InstallState> {
        let context = central.context().await?;
        Ok(context.install_state(&target(OFFICIAL, "video"))?)
    };
    let installed = |update: Option<(&str, bool)>| InstallState::Installed {
        version: "1.1.0".to_owned(),
        update: update.map(|(version, content_only)| AvailableUpdate {
            version: version.to_owned(),
            content_only,
        }),
        damaged: false,
    };
    assert_eq!(state().await?, installed(None));
    let current = central.plan(&[(OFFICIAL, "video")]).await?;
    assert_eq!(actions(&current), [("video", AlreadyInstalled)]);
    assert!(current.changes.is_empty());

    for (published, update) in [
        (video("1.2.0"), Some(("1.2.0", false))),
        (
            video("1.1.0").with("description", json!("Corrigé")),
            Some(("1.1.0", true)),
        ),
        (video("1.0.0"), None),
        (video("1.2.0").omitting("blocks/0.json"), None),
    ] {
        central.publish(OFFICIAL, &[published]).await?;
        assert_eq!(state().await?, installed(update), "{update:?}");
        let prepared = central.plan(&[(OFFICIAL, "video")]).await?;
        ensure_plain(&prepared)?;
        let action = if update.is_some() {
            Update
        } else {
            AlreadyInstalled
        };
        assert_eq!(actions(&prepared), [("video", action)]);
    }
    // En publications immuables, un contenu différent à la même version n'est pas une mise à jour.
    let corrected = video("1.1.0").with("description", json!("Corrigé"));
    central.publish(OFFICIAL, &[corrected]).await?;
    let mut context = central.context().await?;
    context.requires_immutable = true;
    let known = context.installed_state("video");
    assert!(known.is_some_and(|state| state.origin_available && state.update.is_none()));
    Ok(())
}

#[tokio::test]
async fn a_lost_copy_is_repaired_from_its_origin_only() -> anyhow::Result<()> {
    let central = Central::with_installed(&[video("1.0.0")]).await?;
    central.publish(THIRD_PARTY, &[video("2.0.0")]).await?;
    std::fs::remove_dir_all(central.frozen("video").await?)?;
    central.state.catalog_cache.invalidate().await;
    let known = central.context().await?.installed_state("video");
    assert!(known.is_some_and(|state| state.damaged && state.update.is_none()));
    let prepared = central.plan(&[(OFFICIAL, "video")]).await?;
    ensure_plain(&prepared)?;
    assert_eq!(actions(&prepared), [("video", Repair)]);

    // Son dépôt ne publie plus que la version suivante.
    central.publish(OFFICIAL, &[video("1.1.0")]).await?;
    let prepared = central.plan(&[(OFFICIAL, "video")]).await?;
    assert_eq!(actions(&prepared), [("video", Update)]);

    // Une dépendance à réparer l'est avec ce qui l'exige.
    let flow = Item::flow("flux", &["video.run"]);
    central.publish(OFFICIAL, &[video("1.0.0"), flow]).await?;
    let prepared = central.plan(&[(OFFICIAL, "flux")]).await?;
    assert_eq!(actions(&prepared), [("flux", Install), ("video", Repair)]);
    let held = central.plan(&[(THIRD_PARTY, "video")]).await?;
    assert!(matches!(
        held.plan.blockers.as_slice(),
        [PlanBlocker::HeldByOther { .. }]
    ));
    Ok(())
}

#[tokio::test]
async fn several_updates_make_one_plan() -> anyhow::Result<()> {
    let audio = |version: &str| {
        Item::plugin("audio", primitive())
            .version(version)
            .depending(json!([{"id": "ffmpeg", "versionRange": format!("^{version}")}]))
    };
    let ffmpeg = |version: &str| Item::tool("ffmpeg").version(version);
    let central = Central::with_installed(&[audio("1.0.0"), ffmpeg("1.0.0")]).await?;
    central
        .publish(OFFICIAL, &[audio("2.0.0"), ffmpeg("2.0.0")])
        .await?;
    // Ensemble, ou l'un amené par l'autre : les versions se conviennent.
    for targets in [
        &[(OFFICIAL, "audio"), (OFFICIAL, "ffmpeg")][..],
        &[(OFFICIAL, "audio")],
    ] {
        let prepared = central.plan(targets).await?;
        ensure_plain(&prepared)?;
        assert_eq!(actions(&prepared), [("audio", Update), ("ffmpeg", Update)]);
        assert!(change(&prepared, "ffmpeg")?.origin.repository_id == OFFICIAL);
    }
    // Seul, l'outil romprait le plugin installé.
    let prepared = central.plan(&[(OFFICIAL, "ffmpeg")]).await?;
    assert_eq!(
        prepared.plan.blockers,
        [PlanBlocker::BreaksDependents {
            id: "ffmpeg".to_owned(),
            dependents: vec!["audio".to_owned()],
        }]
    );
    Ok(())
}

#[tokio::test]
async fn an_update_must_still_suit_operations_and_scripts() -> anyhow::Result<()> {
    let outil = |version: &str| Item::tool("interpreteurs").version(version);
    let script = Item::script(
        "route",
        script_requiring(json!([{"variable": "media.codec", "provider": "video"}])),
    );
    let exposing = |contract: &str| {
        video("1.0.0")
            .version(contract)
            .exposing("media", contract, &["codec"])
    };
    let central =
        Central::with_installed(&[outil("1.1.0"), exposing("1.0.0"), script.clone()]).await?;
    central
        .publish(OFFICIAL, &[outil("2.0.0"), exposing("2.0.0"), script])
        .await?;
    for id in ["interpreteurs", "video"] {
        let prepared = central.plan(&[(OFFICIAL, id)]).await?;
        assert_eq!(
            prepared.plan.blockers,
            [PlanBlocker::BreaksDependents {
                id: id.to_owned(),
                dependents: vec!["route".to_owned()],
            }]
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_removed_block_is_announced_and_refused_to_unfinished_tasks() -> anyhow::Result<()> {
    let outils = |with_second: bool, version: &str| {
        let plugin = Item::plugin("outils", primitive()).version(version);
        if with_second {
            plugin.block("outils.bis", "outils-bis", "operation.outils.run")
        } else {
            plugin
        }
    };
    let central = Central::with_installed(&[outils(true, "1.0.0")]).await?;
    central.publish(OFFICIAL, &[outils(false, "1.1.0")]).await?;
    // Personne ne s'en sert.
    central.save_flow("flux.autre", &["outils.run"]).await?;
    ensure_plain(&central.plan(&[(OFFICIAL, "outils")]).await?)?;

    central.save_flow("flux.bis", &["outils-bis"]).await?;
    let before = central.plan(&[(OFFICIAL, "outils")]).await?.plan;
    assert_eq!(
        before.consents,
        [PlanConsent::Removes {
            id: "outils".to_owned(),
            removed: "outils-bis".to_owned(),
            flows: vec!["flux.bis".to_owned()],
        }]
    );
    assert!(before.blockers.is_empty());

    central.queue("autre.mkv", "flux.bis").await?;
    let blocked = central.plan(&[(OFFICIAL, "outils")]).await?.plan;
    assert_eq!(
        blocked.blockers,
        [PlanBlocker::InUseByJobs {
            id: "outils".to_owned(),
            removed: "outils-bis".to_owned(),
            jobs: vec!["autre.mkv".to_owned()],
        }]
    );
    assert_ne!(blocked.plan_id, before.plan_id);
    Ok(())
}

#[tokio::test]
async fn an_update_keeps_the_last_probe_of_the_engine() -> anyhow::Result<()> {
    let central = Central::with_installed(&[video("1.0.0")]).await?;
    let without_probe = Item::plugin("video", binary("x")).version("1.1.0");
    central.publish(OFFICIAL, &[without_probe]).await?;
    let prepared = central.plan(&[(OFFICIAL, "video")]).await?;
    assert_eq!(prepared.plan.blockers.len(), 2);
    assert!(prepared.plan.blockers.iter().all(|blocker| matches!(
        blocker,
        PlanBlocker::InvalidItem { id, message } if id == "video" && message.contains("dernière")
    )));
    Ok(())
}

#[tokio::test]
async fn an_unchanged_script_is_updated_and_a_modified_one_is_kept() -> anyhow::Result<()> {
    let script = |version: &str| Item::script("route", primitive()).version(version);
    let central = Central::with_installed(&[script("1.0.0")]).await?;
    central.publish(OFFICIAL, &[script("1.1.0")]).await?;
    let prepared = central.plan(&[(OFFICIAL, "route")]).await?;
    ensure_plain(&prepared)?;
    assert_eq!(actions(&prepared), [("route", Update)]);

    let copy = local_catalog_dir(&central.state.config).join("installed-scripts/route");
    let operation = copy.join("operations/0.json");
    let text = std::fs::read_to_string(&operation)?;
    std::fs::write(&operation, text.replace("validated", "draft"))?;
    let prepared = central.plan(&[(OFFICIAL, "route")]).await?;
    assert_eq!(
        prepared.plan.blockers,
        [PlanBlocker::ModifiedCopy {
            id: "route".to_owned()
        }]
    );
    // Sa copie effacée est à réparer.
    std::fs::remove_dir_all(&copy)?;
    central.publish(OFFICIAL, &[script("1.0.0")]).await?;
    let prepared = central.plan(&[(OFFICIAL, "route")]).await?;
    assert_eq!(actions(&prepared), [("route", Repair)]);
    assert!(prepared.plan.blockers.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_updated_flow_is_a_new_flow_and_moves_nothing() -> anyhow::Result<()> {
    let flow = |version: &str| Item::flow("modele", &["video.run"]).version(version);
    let central = Central::with_installed(&[video("1.0.0"), flow("1.0.0")]).await?;
    central
        .publish(OFFICIAL, &[video("1.0.0"), flow("1.1.0")])
        .await?;
    let prepared = central.plan(&[(OFFICIAL, "modele")]).await?;
    ensure_plain(&prepared)?;
    assert_eq!(
        actions(&prepared),
        [("modele", Update), ("video", AlreadyInstalled)]
    );
    assert_eq!(item(&prepared, "modele")?.version, "1.1.0");
    assert_eq!(
        repository::list_installations(&central.state.pool)
            .await?
            .len(),
        2
    );
    Ok(())
}
