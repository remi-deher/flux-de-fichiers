use super::*;
use crate::flows::OperationRegistry;
use crate::plugins::{CatalogItemType, load_catalog, vocab::PluginCategory};
use forgearr_shared::{
    base64,
    local::{
        CreateBlockTemplateRequest, CreateFlowTemplateRequest, CreateLocalPackRequest,
        ScriptInterpreter, UploadLocalResourceRequest, UpsertLocalScriptRequest,
    },
    plugins::PackComponent,
    script_variables::{ScriptVariable, VariableKind},
};
use serde_json::json;

#[test]
fn local_script_upload_templates_and_pack_roundtrip_without_github() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");

    let script = save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            variables: Vec::new(),
            id: None,
            name: "Tag MKV".into(),
            description: Some("Ajoute un tag".into()),
            interpreter: ScriptInterpreter::Sh,
            code: "cat >/dev/null\nprintf '{\"status\":\"ok\"}'".into(),
            filename: None,
            category: Some(PluginCategory::Media),
            tags: vec!["mkv".into()],
        },
    )?;
    assert_eq!(script.id, "script.tag-mkv");
    assert_eq!(script.entrypoint, "run.sh");

    let sub = upload_local_resource(
        &local_dir,
        UploadLocalResourceRequest {
            id: None,
            name: "Sous-titres FR".into(),
            entry_type: CatalogItemType::ResourceSubtitle,
            filename: "fr.srt".into(),
            content_base64: base64::encode(b"1\n00:00:01,000 --> 00:00:02,000\nSalut\n"),
            description: Some("Piste FR".into()),
        },
    )?;
    assert_eq!(sub.id, "resource.sous-titres-fr");
    let (file_path, download_name) = local_resource_file(&local_dir, &sub.id)?;
    assert_eq!(download_name, "fr.srt");
    assert!(file_path.is_file());

    let block_tpl = save_local_block_template(
        &local_dir,
        CreateBlockTemplateRequest {
            id: None,
            name: "Chaîne Remux".into(),
            description: None,
            definition: json!({"nodes": [], "connections": []}),
            tags: vec![],
        },
    )?;
    assert_eq!(block_tpl.id, "template.block.chaine-remux");

    let flow_tpl = save_local_flow_template(
        &local_dir,
        CreateFlowTemplateRequest {
            id: None,
            name: "Modèle HEVC".into(),
            description: Some("Pipeline type".into()),
            file_mask: Some("*.mkv".into()),
            definition: json!({"nodes": [], "connections": []}),
        },
    )?;
    assert_eq!(flow_tpl.id, "flow.template.modele-hevc");

    let pack = save_local_pack(
        &local_dir,
        CreateLocalPackRequest {
            id: None,
            name: "Mon Pack".into(),
            description: None,
            components: vec![PackComponent {
                id: script.id.clone(),
                version: "1.0.0".into(),
                kind: CatalogItemType::Script,
            }],
            entrypoint: None,
        },
    )?;
    assert_eq!(pack.id, "pack.local.mon-pack");

    let index = load_catalog(&local_dir)?;
    let script_entry = index
        .entries
        .iter()
        .find(|e| e.id == script.id)
        .ok_or_else(|| anyhow::anyhow!("script manquant dans l'index"))?;
    assert_eq!(script_entry.category, Some(PluginCategory::Media));
    assert!(script_entry.sha256.is_some());
    assert!(
        index
            .files_sha256
            .contains_key("scripts/tag-mkv/operations/run.json")
    );
    assert!(index.files_sha256.contains_key("scripts/tag-mkv/run.sh"));
    assert!(
        index
            .files_sha256
            .contains_key("resources/sous-titres-fr/fr.srt")
    );
    assert!(
        index
            .files_sha256
            .contains_key("packs/mon-pack/installation.json")
    );

    let roots = [local_dir.clone()];
    let reg = OperationRegistry::load_from_roots(&roots);
    assert!(reg.get("operation.script.tag-mkv").is_some());

    let snap = build_local_snapshot(&roots, &local_dir);
    assert_eq!(snap.scripts.len(), 1);
    assert_eq!(snap.resources.len(), 1);
    assert_eq!(snap.block_templates.len(), 1);
    assert_eq!(snap.flow_templates.len(), 1);
    assert_eq!(snap.packs.len(), 1);

    delete_local_item(&local_dir, &sub.id)?;
    let snap_after = build_local_snapshot(&roots, &local_dir);
    assert!(snap_after.resources.is_empty());
    assert!(matches!(
        local_resource_file(&local_dir, &sub.id),
        Err(AppError::NotFound)
    ));
    Ok(())
}

#[test]
fn duplicate_creation_without_id_returns_conflict_and_updates_clean_old_files() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");

    let created = save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            variables: Vec::new(),
            id: None,
            name: "Nettoyeur".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: "echo ok".into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;
    assert!(local_dir.join("scripts/nettoyeur/run.sh").is_file());

    let dup = save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            variables: Vec::new(),
            id: None,
            name: "Nettoyeur!".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: "echo dup".into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    );
    assert!(matches!(dup, Err(AppError::Conflict)));

    let updated = save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            variables: Vec::new(),
            id: Some(created.id),
            name: "Nettoyeur Python".into(),
            description: None,
            interpreter: ScriptInterpreter::Python3,
            code: "print('{\"status\":\"ok\"}')".into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    )?;
    assert_eq!(updated.entrypoint, "run.py");
    assert!(!local_dir.join("scripts/nettoyeur/run.sh").exists());
    assert!(local_dir.join("scripts/nettoyeur/run.py").is_file());

    let res = upload_local_resource(
        &local_dir,
        UploadLocalResourceRequest {
            id: None,
            name: "Logo".into(),
            entry_type: CatalogItemType::ResourceImage,
            filename: "logo.png".into(),
            content_base64: base64::encode(b"PNGDATA"),
            description: None,
        },
    )?;
    let dup_res = upload_local_resource(
        &local_dir,
        UploadLocalResourceRequest {
            id: None,
            name: "Logo".into(),
            entry_type: CatalogItemType::ResourceImage,
            filename: "other.png".into(),
            content_base64: base64::encode(b"PNG2"),
            description: None,
        },
    );
    assert!(matches!(dup_res, Err(AppError::Conflict)));

    upload_local_resource(
        &local_dir,
        UploadLocalResourceRequest {
            id: Some(res.id.clone()),
            name: "Logo".into(),
            entry_type: CatalogItemType::ResourceImage,
            filename: "logo-v2.png".into(),
            content_base64: base64::encode(b"PNGV2"),
            description: None,
        },
    )?;
    assert!(!local_dir.join("resources/logo/logo.png").exists());
    assert!(local_dir.join("resources/logo/logo-v2.png").is_file());
    assert_eq!(local_resource_file(&local_dir, &res.id)?.1, "logo-v2.png");
    Ok(())
}

#[test]
fn validation_rejects_mismatched_prefix_invalid_graph_and_invalid_pack_dep() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");

    let bad_prefix = save_local_script(
        &local_dir,
        UpsertLocalScriptRequest {
            variables: Vec::new(),
            id: Some("pack.local.wrong".into()),
            name: "Wrong".into(),
            description: None,
            interpreter: ScriptInterpreter::Sh,
            code: "echo".into(),
            filename: None,
            category: None,
            tags: vec![],
        },
    );
    assert!(matches!(bad_prefix, Err(AppError::InvalidManifest(_))));

    let bad_graph = save_local_block_template(
        &local_dir,
        CreateBlockTemplateRequest {
            id: None,
            name: "Cassé".into(),
            description: None,
            definition: json!({
                "nodes": [],
                "connections": [{ "from": "a", "to": "b" }]
            }),
            tags: vec![],
        },
    );
    assert!(matches!(bad_graph, Err(AppError::InvalidManifest(_))));

    let bad_pack = save_local_pack(
        &local_dir,
        CreateLocalPackRequest {
            id: None,
            name: "Pack Invalide".into(),
            description: None,
            components: vec![PackComponent {
                id: "pack.other".into(),
                version: "1.0.0".into(),
                kind: CatalogItemType::Pack,
            }],
            entrypoint: None,
        },
    );
    assert!(matches!(bad_pack, Err(AppError::InvalidManifest(_))));

    assert!(matches!(
        local_resource_file(&local_dir, "../etc/passwd"),
        Err(AppError::NotFound)
    ));
    Ok(())
}

fn variable_script_request(variables: Vec<ScriptVariable>) -> UpsertLocalScriptRequest {
    UpsertLocalScriptRequest {
        variables,
        id: None,
        name: "Encodeur".into(),
        description: None,
        interpreter: ScriptInterpreter::Sh,
        code: "cat >/dev/null".into(),
        filename: None,
        category: None,
        tags: vec![],
    }
}

fn crf_variable() -> ScriptVariable {
    ScriptVariable {
        name: "crf".into(),
        label: "Qualité".into(),
        description: Some("Plus bas = meilleure qualité".into()),
        kind: VariableKind::Number,
        default_value: Some(json!(23)),
        required: true,
        options: Vec::new(),
        min: Some(0.into()),
        max: Some(51.into()),
    }
}

#[test]
fn script_variables_round_trip_through_the_manifest_and_the_snapshot() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");

    let saved = save_local_script(&local_dir, variable_script_request(vec![crf_variable()]))?;
    assert_eq!(saved.variables, vec![crf_variable()]);

    let snapshot = build_local_snapshot(std::slice::from_ref(&local_dir), &local_dir);
    assert_eq!(snapshot.scripts.len(), 1);
    assert_eq!(snapshot.scripts[0].variables, vec![crf_variable()]);

    let plain_dir = dir.path().join("plain");
    save_local_script(&plain_dir, variable_script_request(Vec::new()))?;
    let plain = build_local_snapshot(std::slice::from_ref(&plain_dir), &plain_dir);
    assert!(plain.scripts[0].variables.is_empty());
    Ok(())
}

#[test]
fn script_with_an_invalid_variable_schema_is_refused_and_writes_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let local_dir = dir.path().join("local");

    let mut bad = crf_variable();
    bad.default_value = Some(json!(99));
    let refused = save_local_script(&local_dir, variable_script_request(vec![bad]));
    assert!(matches!(refused, Err(AppError::InvalidManifest(_))));
    assert!(!local_dir.join("scripts/encodeur").exists());
    Ok(())
}
