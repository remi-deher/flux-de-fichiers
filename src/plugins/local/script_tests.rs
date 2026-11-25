//! Un script installé s'édite comme un script de l'utilisateur, sans perdre ce que l'éditeur ne montre pas.

use super::*;
use crate::{
    flows::{OperationRegistry, OperationRuntime},
    plugins::load_catalog,
};
use forgearr_shared::{
    exposed_variables::ScriptRequirement,
    local::{LocalScriptSummary, ScriptInterpreter, UpsertLocalScriptRequest},
    script_variables::{ScriptVariable, VariableKind},
};
use serde_json::json;

const ID: &str = "route";
const OPERATION: &str = "operation.script.route";
const CODE: &str = "print('route')\n";
const HELPER: &str = "aide.py";

fn dir(local_dir: &Path) -> PathBuf {
    local_dir.join(INSTALLED_SCRIPTS_FOLDER).join(ID)
}

fn manifest() -> Value {
    json!({
        "schemaVersion": "1.1.0", "id": ID, "type": "script", "version": "1.4.0",
        "name": "Routage : format HDR", "description": "Oriente le flux.",
        "author": "Forgearr", "license": "GPL-2.0-or-later", "category": "hdr",
        "capability": "script", "tags": ["hdr"],
        "dependencies": [{"id": "interpreteurs", "kind": "app"}],
        "files": [
            {"path": "README.md", "role": "docs", "mediaType": "text/markdown"},
            {"path": "run.py", "role": "script", "mediaType": "text/plain"},
            {"path": HELPER, "role": "script", "mediaType": "text/plain"},
            {"path": "operations/run.json", "role": "operation", "mediaType": "application/json"},
            {"path": "blocks/run.json", "role": "block", "mediaType": "application/json"}
        ]
    })
}

fn operation() -> Value {
    json!({
        "schemaVersion": "1.1.0", "id": OPERATION, "version": "1.2.0", "name": "Routage",
        "providerId": ID, "providerKind": "script", "capability": "script", "status": "validated",
        "runtime": {
            "kind": "script", "tool": {"id": "interpreteurs", "version": "1.1.0"},
            "pathTools": [{"id": "mediainfo", "version": "1.0.0"}],
            "interpreter": "python3", "executable": "python3.12", "entrypoint": "run.py",
            "args": ["--strict"], "timeoutSeconds": 60, "acceptsNonMedia": true,
            "defaults": {"target_format": "any-dynamic"}
        }
    })
}

fn block() -> Value {
    json!({
        "schemaVersion": "1.1.0", "id": "route.run", "version": "1.0.0", "name": "Routage : HDR",
        "description": "Oriente selon le format.", "category": "media", "aliases": ["route-run"],
        "operationId": OPERATION, "operationVersion": "1.2.0",
        "ui": {
            "category": "Scripts", "domain": "hdr", "icon": "fas fa-code-branch",
            "ports": [{"id": "true", "label": "Oui"}, {"id": "false", "label": "Non"}],
            "fields": [{"name": "target_format", "label": "Format", "type": "select", "default": "any-dynamic"}]
        }
    })
}

fn install(local_dir: &Path) -> anyhow::Result<()> {
    let dir = dir(local_dir);
    for folder in ["operations", "blocks"] {
        fs::create_dir_all(dir.join(folder))?;
    }
    fs::write(dir.join("manifest.json"), manifest().to_string())?;
    fs::write(dir.join("operations/run.json"), operation().to_string())?;
    fs::write(dir.join("blocks/run.json"), block().to_string())?;
    fs::write(dir.join("run.py"), CODE)?;
    fs::write(dir.join(HELPER), "# aide\n")?;
    rebuild_local_index(local_dir)?;
    Ok(())
}

fn read(local_dir: &Path, file: &str) -> anyhow::Result<Value> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dir(local_dir).join(file),
    )?)?)
}

/// Le script tel que l'éditeur l'ouvre.
fn opened(local_dir: &Path) -> anyhow::Result<UpsertLocalScriptRequest> {
    let snapshot = build_local_snapshot(&[local_dir.to_path_buf()], local_dir);
    let script: &LocalScriptSummary = snapshot
        .scripts
        .iter()
        .find(|script| script.id == ID)
        .ok_or_else(|| anyhow::anyhow!("script absent de la collection"))?;
    Ok(UpsertLocalScriptRequest {
        id: Some(script.id.clone()),
        name: script.name.clone(),
        description: script.description.clone(),
        interpreter: script.interpreter,
        code: script.code.clone(),
        filename: Some(script.entrypoint.clone()),
        category: script.category,
        tags: script.tags.clone(),
        variables: script.variables.clone(),
        requires: script.requires.clone(),
        dependencies_unverified: script.dependencies_unverified,
    })
}

#[test]
fn saving_an_installed_script_unchanged_rewrites_nothing() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    install(local.path())?;
    let request = opened(local.path())?;
    assert_eq!(request.code, CODE);
    // L'éditeur n'envoie pas une catégorie qu'il ne connaît pas.
    let request = UpsertLocalScriptRequest {
        category: None,
        ..request
    };
    save_local_script(local.path(), request)?;
    for (file, expected) in [
        ("manifest.json", manifest().to_string()),
        ("operations/run.json", operation().to_string()),
        ("blocks/run.json", block().to_string()),
        ("run.py", CODE.to_owned()),
    ] {
        let written = fs::read_to_string(dir(local.path()).join(file))?;
        assert_eq!(written, expected, "{file}");
    }
    Ok(())
}

#[test]
fn editing_an_installed_script_keeps_what_the_editor_does_not_show() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    install(local.path())?;
    let variable = ScriptVariable {
        name: "seuil".into(),
        kind: VariableKind::Number,
        ..ScriptVariable::default()
    };
    let requirement = ScriptRequirement {
        variable: "mediainfo.video.codec".into(),
        provider: "mediainfo".into(),
        contract_major: 1,
        repository: None,
    };
    let saved = save_local_script(
        local.path(),
        UpsertLocalScriptRequest {
            code: "print('modifié')\n".into(),
            tags: vec!["perso".into()],
            variables: vec![variable.clone()],
            requires: vec![requirement.clone()],
            ..opened(local.path())?
        },
    )?;
    assert_eq!(saved.id, ID);
    assert!(!local.path().join("scripts").exists());

    let mut expected = manifest();
    expected["tags"] = json!(["perso"]);
    assert_eq!(read(local.path(), "manifest.json")?, expected);
    assert_eq!(read(local.path(), "blocks/run.json")?, block());
    let mut expected = operation();
    expected["runtime"]["variables"] = json!([variable]);
    expected["runtime"]["requires"] = json!([requirement]);
    assert_eq!(read(local.path(), "operations/run.json")?, expected);

    // Les flux qui s'en servent gardent leur bloc, leur alias et leur opération.
    let registry = OperationRegistry::load_from_roots(&[local.path().to_path_buf()]);
    for kind in ["route.run", "route-run"] {
        assert_eq!(registry.operation_id_for_kind(kind), Some(OPERATION));
    }
    let Some(OperationRuntime::Script(script)) = registry.get(OPERATION).map(|op| &op.runtime)
    else {
        anyhow::bail!("opération absente");
    };
    assert_eq!(script.code.as_deref(), Some("print('modifié')\n"));
    assert_eq!(script.timeout_seconds, 60);
    assert!(script.accepts_non_media);

    // Remis comme avant, il l'est.
    save_local_script(
        local.path(),
        UpsertLocalScriptRequest {
            variables: Vec::new(),
            requires: Vec::new(),
            tags: vec!["hdr".into()],
            ..opened(local.path())?
        },
    )?;
    assert_eq!(read(local.path(), "manifest.json")?, manifest());
    assert_eq!(read(local.path(), "operations/run.json")?, operation());
    Ok(())
}

#[test]
fn renaming_and_changing_the_language_follow_everywhere() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    install(local.path())?;
    let saved = save_local_script(
        local.path(),
        UpsertLocalScriptRequest {
            name: "Mon routage".into(),
            description: Some("À moi.".into()),
            interpreter: ScriptInterpreter::Sh,
            code: "echo ok".into(),
            filename: None,
            ..opened(local.path())?
        },
    )?;
    assert_eq!(saved.entrypoint, "run.sh");
    assert!(!dir(local.path()).join("run.py").exists());
    assert!(dir(local.path()).join("run.sh").is_file());
    for file in ["manifest.json", "operations/run.json", "blocks/run.json"] {
        assert_eq!(read(local.path(), file)?["name"], "Mon routage", "{file}");
    }
    assert_eq!(
        read(local.path(), "blocks/run.json")?["description"],
        "À moi."
    );
    let manifest = read(local.path(), "manifest.json")?;
    assert_eq!(manifest["files"][1]["path"], "run.sh");
    let operation = read(local.path(), "operations/run.json")?;
    let runtime = &operation["runtime"];
    assert_eq!(runtime["interpreter"], "sh");
    assert_eq!(runtime["tool"]["id"], "interpreteurs");
    assert!(runtime.get("executable").is_none());
    let index = load_catalog(local.path())?;
    let hashed = format!("{INSTALLED_SCRIPTS_FOLDER}/{ID}/run.sh");
    assert!(index.files_sha256.contains_key(&hashed));
    Ok(())
}

#[test]
fn the_code_of_a_script_never_replaces_another_of_its_files() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    install(local.path())?;
    for taken in [HELPER, "manifest.json", "../run.py"] {
        let refused = save_local_script(
            local.path(),
            UpsertLocalScriptRequest {
                filename: Some(taken.into()),
                ..opened(local.path())?
            },
        );
        assert!(
            matches!(refused, Err(AppError::InvalidManifest(_))),
            "{taken}"
        );
    }
    assert_eq!(read(local.path(), "manifest.json")?, manifest());
    assert_eq!(
        fs::read_to_string(dir(local.path()).join(HELPER))?,
        "# aide\n"
    );
    Ok(())
}

#[test]
fn a_new_script_cannot_take_the_operation_of_an_installed_one() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    install(local.path())?;
    let request = UpsertLocalScriptRequest {
        name: "Route".into(),
        code: "echo".into(),
        ..UpsertLocalScriptRequest::default()
    };
    assert!(script_name_taken(local.path(), "Route"));
    let refused = save_local_script(local.path(), request);
    assert!(matches!(refused, Err(AppError::Conflict)));
    assert!(!local.path().join("scripts").exists());
    Ok(())
}

#[test]
fn code_carried_by_the_operation_moves_to_its_file() -> anyhow::Result<()> {
    let local = tempfile::tempdir()?;
    install(local.path())?;
    let mut inline = operation();
    inline["runtime"]["code"] = json!("print('en ligne')");
    fs::write(
        dir(local.path()).join("operations/run.json"),
        inline.to_string(),
    )?;
    let request = opened(local.path())?;
    assert_eq!(request.code, "print('en ligne')");
    save_local_script(local.path(), request)?;
    assert_eq!(read(local.path(), "operations/run.json")?, operation());
    assert_eq!(opened(local.path())?.code, "print('en ligne')");
    Ok(())
}
