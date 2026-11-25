//! Ce que chaque type d'élément ajoute au plan.

use super::{
    OFFICIAL, THIRD_PARTY, actions, change, ensure_plain,
    fixture::{Central, target},
    item,
    item::{Item, base, binary, primitive, script_requiring},
    tool,
};
use crate::error::AppError;
use forgearr_shared::{
    Permission,
    plugins::{
        ToolStatus,
        store::{
            InstallState,
            plan::{
                PlanAction::{AlreadyInstalled, Install},
                PlanBlocker, PlanReason, PlanWarning,
            },
        },
    },
};
use serde_json::json;

fn video() -> Item {
    Item::plugin("video", base())
        .operation("operation.video.inspect", binary("mediainfo"))
        .recommending(&["ffmpeg", "mkvtoolnix", "absent"])
        .with(
            "settingsSchema",
            json!({"fields": [
                {"name": "url", "label": "URL", "type": "url", "required": true}
            ]}),
        )
}

fn essential() -> Item {
    Item::plugin("essentiel", primitive()).version("1.3.0")
}

#[tokio::test]
async fn a_plugin_brings_its_tools_checked_when_they_are_present() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let tools = ["ffmpeg", "mkvtoolnix", "mediainfo"].map(Item::tool);
    let items: Vec<Item> = [video()].into_iter().chain(tools).collect();
    central.publish(OFFICIAL, &items).await?;
    let prepared = central.plan(&[(OFFICIAL, "video")]).await?;
    assert_eq!(
        actions(&prepared),
        [
            ("video", Install),
            ("ffmpeg", Install),
            ("mkvtoolnix", Install),
            ("mediainfo", Install)
        ]
    );
    for id in ["ffmpeg", "mkvtoolnix", "mediainfo"] {
        let tool = tool(&prepared, id)?;
        assert!(tool.preselected && tool.installable, "{id}");
        assert_eq!(tool.status, ToolStatus::NotInstalled);
        assert_eq!(tool.required_by, ["video"]);
    }
    let plugin = item(&prepared, "video")?;
    assert_eq!(plugin.reason, PlanReason::Target);
    assert!(plugin.has_settings && !plugin.configured);
    assert!(prepared.plan.blockers.is_empty() && prepared.plan.consents.is_empty());
    assert_eq!(
        prepared.plan.warnings,
        [PlanWarning::OptionalMissing {
            id: "absent".to_owned(),
            required_by: "video".to_owned(),
        }]
    );
    assert_eq!(prepared.changes.len(), 4);
    assert!(change(&prepared, "video")?.content.missing.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_identity_of_a_plan_follows_its_content() -> anyhow::Result<()> {
    let central = Central::new().await?;
    central.publish(OFFICIAL, &[essential()]).await?;
    let targets = [(OFFICIAL, "essentiel")];
    let first = central.plan(&targets).await?.plan.plan_id;
    assert_eq!(central.plan(&targets).await?.plan.plan_id, first);
    let changed = essential().with("description", json!("Autre"));
    central.publish(OFFICIAL, &[changed]).await?;
    assert_ne!(central.plan(&targets).await?.plan.plan_id, first);
    Ok(())
}

#[tokio::test]
async fn a_tool_brings_the_tools_it_needs_even_in_a_cycle() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let ffmpeg = Item::tool("ffmpeg").depending(json!([{"id": "libs"}]));
    let libs = Item::tool("libs").depending(json!([{"id": "ffmpeg", "versionRange": "^1.0.0"}]));
    let preset = Item::new("preset", "reglage");
    central.publish(OFFICIAL, &[ffmpeg, libs, preset]).await?;
    let prepared = central.plan(&[(OFFICIAL, "ffmpeg")]).await?;
    ensure_plain(&prepared)?;
    assert_eq!(actions(&prepared), [("ffmpeg", Install), ("libs", Install)]);
    assert_eq!(item(&prepared, "ffmpeg")?.required_by, ["libs"]);
    assert_eq!(prepared.plan.tools.len(), 2);
    // Un préréglage ne dépend de rien ; une cible répétée ne compte qu'une fois.
    let prepared = central
        .plan(&[(OFFICIAL, "reglage"), (OFFICIAL, "reglage")])
        .await?;
    ensure_plain(&prepared)?;
    assert_eq!(actions(&prepared), [("reglage", Install)]);
    Ok(())
}

#[tokio::test]
async fn a_flow_brings_the_owners_of_its_blocks_and_the_baseline() -> anyhow::Result<()> {
    let central = Central::new().await?;
    // `essentiel` et `video` ne sont pas dans ses dépendances ; l'alias suffit.
    let audio = Item::plugin("audio", primitive());
    let flow = Item::flow("conversion", &["essentiel-run", "audio.run"])
        .depending(json!([{"id": "audio", "versionRange": "^1.0.0", "kind": "plugin"}]));
    central
        .publish(OFFICIAL, &[flow, essential(), audio, video()])
        .await?;
    let prepared = central.plan(&[(OFFICIAL, "conversion")]).await?;
    assert_eq!(
        actions(&prepared),
        [
            ("conversion", Install),
            ("audio", Install),
            ("essentiel", Install),
            ("video", Install)
        ]
    );
    assert_eq!(item(&prepared, "video")?.reason, PlanReason::Baseline);
    assert_eq!(item(&prepared, "essentiel")?.reason, PlanReason::Required);
    assert!(prepared.plan.blockers.is_empty());

    let refused = central.plan_as(&[(OFFICIAL, "conversion")], false).await?;
    assert_eq!(
        refused.plan.blockers,
        [PlanBlocker::MissingPermission {
            permission: Permission::ManageFlows
        }]
    );
    assert!(
        central
            .plan_as(&[(OFFICIAL, "audio")], false)
            .await?
            .plan
            .blockers
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn a_flow_uses_what_the_engine_already_has() -> anyhow::Result<()> {
    let central = Central::with_installed(&[essential(), video()]).await?;
    let flow = Item::flow("conversion", &["essentiel.run", "inconnu.run"]);
    central.publish(THIRD_PARTY, &[flow]).await?;
    let prepared = central.plan(&[(THIRD_PARTY, "conversion")]).await?;
    assert_eq!(
        actions(&prepared),
        [("conversion", Install), ("essentiel", AlreadyInstalled)]
    );
    assert_eq!(
        prepared.plan.blockers,
        [PlanBlocker::Missing {
            id: "inconnu.run".to_owned(),
            required_by: "conversion".to_owned(),
        }]
    );
    let empty = Item::flow("vide", &[]);
    central.publish(THIRD_PARTY, &[empty]).await?;
    let prepared = central.plan(&[(THIRD_PARTY, "vide")]).await?;
    assert!(matches!(
        prepared.plan.blockers.as_slice(),
        [PlanBlocker::InvalidItem { .. }]
    ));
    assert!(prepared.changes.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_script_brings_the_tools_and_providers_it_does_not_declare() -> anyhow::Result<()> {
    let central = Central::new().await?;
    let repository = json!({"id": "repo.exif", "indexUrl": "https://exif.example/index.json"});
    let script = Item::script(
        "route",
        script_requiring(json!([
            {"variable": "mediainfo.codec", "provider": "video"},
            {"variable": "exif.model", "provider": "exif", "repository": repository}
        ])),
    );
    let provider = Item::plugin("video", base()).exposing("mediainfo", "1.0.0", &["codec"]);
    let tool_item = Item::tool("interpreteurs").version("1.1.0");
    central
        .publish(OFFICIAL, &[script.clone(), provider, tool_item])
        .await?;
    let prepared = central.plan(&[(OFFICIAL, "route")]).await?;
    assert_eq!(
        actions(&prepared),
        [
            ("route", Install),
            ("interpreteurs", Install),
            ("video", Install)
        ]
    );
    assert_eq!(tool(&prepared, "interpreteurs")?.version, "1.1.0");
    assert_eq!(
        prepared.plan.blockers,
        [PlanBlocker::Missing {
            id: "exif".to_owned(),
            required_by: "route".to_owned(),
        }]
    );
    assert_eq!(prepared.plan.suggested_repositories.len(), 1);

    // Un fournisseur présent qui n'expose pas la variable, et un outil trop ancien.
    let muet = Item::plugin("video", base());
    let exif = Item::plugin("exif", primitive()).exposing("exif", "1.0.0", &["model"]);
    let ancien = Item::tool("interpreteurs");
    central
        .publish(OFFICIAL, &[script, muet, exif, ancien])
        .await?;
    let prepared = central.plan(&[(OFFICIAL, "route")]).await?;
    let blockers = &prepared.plan.blockers;
    assert_eq!(blockers.len(), 2, "{blockers:?}");
    assert!(matches!(
        &blockers[0],
        PlanBlocker::VersionTooOld { id, available, .. }
            if id == "interpreteurs" && available == "1.0.0"
    ));
    assert!(matches!(
        &blockers[1],
        PlanBlocker::InvalidItem { id, message }
            if id == "route" && message.contains("mediainfo.codec")
    ));
    Ok(())
}

#[tokio::test]
async fn a_pack_is_the_sum_of_its_components() -> anyhow::Result<()> {
    let central = Central::with_installed(&[essential()]).await?;
    let pack = Item::pack(
        "pack",
        &[
            ("plugin", "essentiel", "1.0.0"),
            ("app", "ffmpeg", "1.0.0"),
            ("preset", "reglage", "1.0.0"),
        ],
    );
    let ffmpeg = Item::tool("ffmpeg");
    let preset = Item::new("preset", "reglage");
    central
        .publish(OFFICIAL, &[pack, essential(), ffmpeg, preset])
        .await?;
    let pack_ref = target(OFFICIAL, "pack");
    let state = central.context().await?.install_state(&pack_ref)?;
    assert_eq!(state, InstallState::PartlyInstalled { remaining: 2 });

    let prepared = central.plan(&[(OFFICIAL, "pack")]).await?;
    ensure_plain(&prepared)?;
    assert_eq!(
        actions(&prepared),
        [
            ("essentiel", AlreadyInstalled),
            ("ffmpeg", Install),
            ("reglage", Install)
        ]
    );
    assert!(
        prepared
            .plan
            .items
            .iter()
            .all(|item| item.reason == PlanReason::Component && item.required_by == ["pack"])
    );
    for id in ["ffmpeg", "reglage"] {
        central.install(OFFICIAL, id).await?;
    }
    let state = central.context().await?.install_state(&pack_ref)?;
    assert!(matches!(state, InstallState::Installed { .. }));
    Ok(())
}

#[tokio::test]
async fn a_plan_needs_known_targets_and_a_finished_migration() -> anyhow::Result<()> {
    let central = Central::new().await?;
    central.publish(OFFICIAL, &[essential()]).await?;
    for (repository, id) in [(OFFICIAL, "absent"), ("repo.absent", "essentiel")] {
        let unknown = central.plan(&[(repository, id)]).await;
        assert!(matches!(unknown, Err(AppError::NotFound)), "{id}");
    }
    let block = central.plan(&[(OFFICIAL, "essentiel.run")]).await;
    assert!(matches!(block, Err(AppError::Invalid)));
    assert!(matches!(central.plan(&[]).await, Err(AppError::Invalid)));

    let pending = [OFFICIAL.to_owned()];
    crate::repository::set_pending_store_repositories(&central.state.pool, &pending).await?;
    let refused = central.plan(&[(OFFICIAL, "essentiel")]).await;
    assert!(
        matches!(refused, Err(AppError::StoreMigrating(message)) if message.contains(OFFICIAL))
    );
    Ok(())
}
