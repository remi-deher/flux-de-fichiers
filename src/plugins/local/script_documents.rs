//! Documents d'un script de la collection : manifeste, opération, bloc et code. La saisie n'en change que ce
//! qu'elle dit : outil, réglages par défaut, ports, champs, alias ou version d'un script installé restent
//! (ADR 0029).

use super::{
    is_safe_filename, locate::declared_file, normalize_description, operation_id, script_slug,
    write_json_pretty,
};
use crate::{
    error::{AppError, AppResult},
    plugins::{installed::replace_file, vocab::PluginCategory},
    tools_cache::ToolCacheManager,
};
use forgearr_shared::local::{ScriptInterpreter, UpsertLocalScriptRequest};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

const MANIFEST_FILE: &str = "manifest.json";
const OPERATION_FILE: &str = "operations/run.json";
const BLOCK_FILE: &str = "blocks/run.json";
const OPERATION_ROLE: &str = "operation";
const BLOCK_ROLE: &str = "block";
const SCRIPT_ROLE: &str = "script";
const SCRIPT_KIND: &str = "script";
const DEFAULT_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_DESCRIPTION: &str = "Script local personnalisé.";
const DEFAULT_BLOCK_DESCRIPTION: &str = "Script local personnalisé (stdio JSON).";

fn invalid(reason: impl Into<String>) -> AppError {
    AppError::InvalidManifest(reason.into())
}

/// Document JSON ; n'est réécrit que s'il a changé.
struct Document {
    path: PathBuf,
    on_disk: Option<Value>,
    value: Value,
}

impl Document {
    fn new(path: PathBuf, value: Value) -> Self {
        Self {
            path,
            on_disk: None,
            value,
        }
    }

    /// `None` s'il n'existe pas.
    fn read(path: PathBuf) -> AppResult<Option<Self>> {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| invalid(format!("{} : {error}", path.display())))?;
        Ok(Some(Self {
            path,
            on_disk: Some(value.clone()),
            value,
        }))
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.value.get(key).and_then(Value::as_str)
    }

    fn fields(&mut self) -> AppResult<&mut Map<String, Value>> {
        let path = self.path.display().to_string();
        self.value
            .as_object_mut()
            .ok_or_else(|| invalid(format!("{path} : objet attendu")))
    }

    fn write(&self) -> AppResult<()> {
        if self.on_disk.as_ref() == Some(&self.value) {
            return Ok(());
        }
        write_json_pretty(&self.path, &self.value)
    }
}

/// Remplace un champ dont la valeur lue diffère ; à sa valeur par défaut, il est retiré.
fn set_field<T>(fields: &mut Map<String, Value>, key: &str, wanted: &T) -> AppResult<()>
where
    T: Default + PartialEq + Serialize + DeserializeOwned,
{
    let is_default = *wanted == T::default();
    let is_kept = match fields.get(key) {
        Some(raw) => T::deserialize(raw).is_ok_and(|current| current == *wanted),
        None => is_default,
    };
    if is_kept {
        return Ok(());
    }
    if is_default {
        fields.remove(key);
    } else {
        let value = serde_json::to_value(wanted).map_err(|error| invalid(error.to_string()))?;
        fields.insert(key.to_owned(), value);
    }
    Ok(())
}

pub(super) struct ScriptDocuments {
    dir: PathBuf,
    id: String,
    manifest: Document,
    operation: Document,
    /// Un script peut n'avoir aucun bloc à lui.
    block: Option<Document>,
    /// Code que le fichier `entrypoint` remplace.
    stale_entrypoint: Option<String>,
}

impl ScriptDocuments {
    /// Documents d'un nouveau script, sans rien écrire.
    pub(super) fn new(dir: PathBuf, id: String) -> Self {
        let manifest = json!({
            "schemaVersion": "1.1.0",
            "id": id,
            "type": "script",
            "version": "1.0.0",
            "description": DEFAULT_DESCRIPTION,
            "author": "local",
            "license": "UNLICENSED",
            "category": PluginCategory::Other.as_str(),
            "tags": [],
            "files": [
                { "path": OPERATION_FILE, "role": OPERATION_ROLE, "mediaType": "application/json" },
                { "path": BLOCK_FILE, "role": BLOCK_ROLE, "mediaType": "application/json" }
            ]
        });
        Self {
            manifest: Document::new(dir.join(MANIFEST_FILE), manifest),
            operation: new_operation(dir.join(OPERATION_FILE), &id),
            block: Some(new_block(dir.join(BLOCK_FILE), &id)),
            stale_entrypoint: None,
            dir,
            id,
        }
    }

    /// Documents du script de ce dossier ; `None` sans manifeste.
    pub(super) fn load(dir: &Path) -> AppResult<Option<Self>> {
        let Some(manifest) = Document::read(dir.join(MANIFEST_FILE))? else {
            return Ok(None);
        };
        let id = manifest
            .text("id")
            .ok_or_else(|| invalid("script sans identifiant"))?
            .to_owned();
        let operation = dir.join(
            declared_file(&manifest.value, OPERATION_ROLE)
                .ok_or_else(|| invalid("script sans opération déclarée"))?,
        );
        let block = declared_file(&manifest.value, BLOCK_ROLE)
            .map(|path| read_or_new(dir.join(path), &id, new_block))
            .transpose()?;
        Ok(Some(Self {
            dir: dir.to_path_buf(),
            operation: read_or_new(operation, &id, new_operation)?,
            id,
            manifest,
            block,
            stale_entrypoint: None,
        }))
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn description(&self) -> Option<String> {
        normalize_description(self.manifest.text("description"))
    }

    pub(super) fn interpreter(&self) -> Option<ScriptInterpreter> {
        self.runtime_field("interpreter")
    }

    /// Champ de l'opération ; `None` s'il manque ou est illisible.
    pub(super) fn runtime_field<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        T::deserialize(self.operation.value.get("runtime")?.get(key)?).ok()
    }

    /// Fichier du code, relatif au dossier du script.
    pub(super) fn entrypoint(&self) -> Option<String> {
        self.runtime_field::<String>("entrypoint").filter(|path| {
            Path::new(path).file_name().is_some()
                && ToolCacheManager::is_safe_relative_path(Path::new(path))
        })
    }

    /// Code de l'opération, sinon celui de son fichier ; vide s'il manque.
    pub(super) fn code(&self) -> String {
        self.runtime_field::<String>("code")
            .filter(|code| !code.is_empty())
            .or_else(|| fs::read_to_string(self.dir.join(self.entrypoint()?)).ok())
            .unwrap_or_default()
    }

    /// Reporte la saisie dans les documents, sans rien écrire.
    pub(super) fn apply(&mut self, req: &UpsertLocalScriptRequest) -> AppResult<()> {
        let current = self.entrypoint();
        let entrypoint = self.requested_entrypoint(req, current.as_deref())?;
        let is_same_interpreter = self.interpreter() == Some(req.interpreter);
        self.set_name(req.name.trim())?;
        self.set_description(req.description.as_deref())?;
        let manifest = self.manifest.fields()?;
        set_category(manifest, req.category);
        set_field(manifest, "tags", &req.tags)?;
        if current.as_ref() != Some(&entrypoint) {
            declare_entrypoint(manifest, current.as_deref(), &entrypoint);
        }

        let runtime = self
            .operation
            .fields()?
            .get_mut("runtime")
            .and_then(Value::as_object_mut)
            .filter(|runtime| runtime.get("kind").and_then(Value::as_str) == Some(SCRIPT_KIND))
            .ok_or_else(|| invalid("l'opération de ce script n'est pas modifiable"))?;
        if !is_same_interpreter {
            runtime.insert("interpreter".into(), json!(req.interpreter.executable()));
            // Il désignait l'interpréteur précédent.
            runtime.remove("executable");
        }
        if current.as_ref() != Some(&entrypoint) {
            runtime.insert("entrypoint".into(), json!(entrypoint));
        }
        // Le code est dans son fichier.
        runtime.remove("code");
        set_field(runtime, "variables", &req.variables)?;
        set_field(runtime, "requires", &req.requires)?;
        set_field(
            runtime,
            "dependenciesUnverified",
            &req.dependencies_unverified,
        )?;
        self.stale_entrypoint = current.filter(|previous| *previous != entrypoint);
        Ok(())
    }

    /// Écrit ce qui a changé. Le manifeste vient en dernier : sans lui, rien n'est dans la collection.
    pub(super) fn write(&self, code: &str) -> AppResult<()> {
        let entrypoint = self
            .entrypoint()
            .ok_or_else(|| invalid("script sans fichier de code"))?;
        let target = self.dir.join(entrypoint);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::read(&target).ok().as_deref() != Some(code.as_bytes()) {
            replace_file(&target, code.as_bytes())?;
        }
        self.operation.write()?;
        if let Some(block) = &self.block {
            block.write()?;
        }
        self.manifest.write()?;
        if let Some(stale) = &self.stale_entrypoint {
            let _ = fs::remove_file(self.dir.join(stale));
        }
        Ok(())
    }

    fn requested_entrypoint(
        &self,
        req: &UpsertLocalScriptRequest,
        current: Option<&str>,
    ) -> AppResult<String> {
        let typed = req.filename.as_deref().map(str::trim).unwrap_or_default();
        let wanted = match current {
            Some(current) if typed == current => return Ok(current.to_owned()),
            Some(current) if typed.is_empty() && self.interpreter() == Some(req.interpreter) => {
                return Ok(current.to_owned());
            }
            _ if typed.is_empty() => format!("run.{}", req.interpreter.default_extension()),
            _ => typed.to_owned(),
        };
        if !is_safe_filename(&wanted) {
            return Err(invalid("nom de fichier script invalide"));
        }
        if Some(wanted.as_str()) != current && self.holds_file(&wanted) {
            return Err(invalid(format!(
                "le fichier « {wanted} » sert déjà à autre chose"
            )));
        }
        Ok(wanted)
    }

    /// Le script a-t-il déjà ce fichier ?
    fn holds_file(&self, path: &str) -> bool {
        let files = self.manifest.value.get("files").and_then(Value::as_array);
        path == MANIFEST_FILE || files.into_iter().flatten().any(|file| is_at(file, path))
    }

    /// Un nom propre à l'opération ou au bloc reste, sauf si le script change de nom.
    fn set_name(&mut self, name: &str) -> AppResult<()> {
        if self.manifest.text("name").map(str::trim) == Some(name) {
            return Ok(());
        }
        for document in [
            Some(&mut self.manifest),
            Some(&mut self.operation),
            self.block.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            document.fields()?.insert("name".into(), json!(name));
        }
        Ok(())
    }

    fn set_description(&mut self, typed: Option<&str>) -> AppResult<()> {
        let description = normalize_description(typed);
        if self.description() == description {
            return Ok(());
        }
        let described = [
            (Some(&mut self.manifest), DEFAULT_DESCRIPTION),
            (self.block.as_mut(), DEFAULT_BLOCK_DESCRIPTION),
        ];
        for (document, default) in described {
            if let Some(document) = document {
                let text = description.as_deref().unwrap_or(default);
                document.fields()?.insert("description".into(), json!(text));
            }
        }
        Ok(())
    }
}

fn read_or_new(path: PathBuf, id: &str, new: fn(PathBuf, &str) -> Document) -> AppResult<Document> {
    Ok(match Document::read(path.clone())? {
        Some(known) => known,
        None => new(path, id),
    })
}

fn new_operation(path: PathBuf, id: &str) -> Document {
    let operation = json!({
        "schemaVersion": "1.1.0",
        "id": operation_id(script_slug(id)),
        "version": "1.0.0",
        "name": id,
        "providerId": id,
        "providerKind": "script",
        "capability": "script",
        "status": "validated",
        "runtime": { "kind": SCRIPT_KIND, "timeoutSeconds": DEFAULT_TIMEOUT_SECONDS }
    });
    Document::new(path, operation)
}

fn new_block(path: PathBuf, id: &str) -> Document {
    let slug = script_slug(id);
    let block = json!({
        "schemaVersion": "1.1.0",
        "id": format!("block.script.{slug}"),
        "version": "1.0.0",
        "name": id,
        "description": DEFAULT_BLOCK_DESCRIPTION,
        "category": "data",
        "operationId": operation_id(slug),
        "operationVersion": "1.0.0",
        "sockets": {
            "inputs": [{ "id": "source", "label": "Fichier traité", "dataType": "file", "mediaType": "*/*" }],
            "outputs": [{ "id": "destination", "label": "Fichier courant", "dataType": "file", "mediaType": "*/*" }]
        },
        "ui": {
            "category": "Scripts locaux",
            "domain": "data",
            "fields": []
        }
    });
    Document::new(path, block)
}

/// Une catégorie que l'éditeur ne connaît pas reste.
fn set_category(manifest: &mut Map<String, Value>, typed: Option<PluginCategory>) {
    let wanted = typed.unwrap_or(PluginCategory::Other);
    let current = manifest
        .get("category")
        .and_then(Value::as_str)
        .map_or(PluginCategory::Other, PluginCategory::from_label);
    if current != wanted {
        manifest.insert("category".into(), json!(wanted.as_str()));
    }
}

fn is_at(file: &Value, path: &str) -> bool {
    file.get("path").and_then(Value::as_str) == Some(path)
}

/// Déclare le fichier du code, à la place du précédent.
fn declare_entrypoint(manifest: &mut Map<String, Value>, previous: Option<&str>, entrypoint: &str) {
    let files = manifest.entry("files").or_insert_with(|| json!([]));
    let Some(files) = files.as_array_mut() else {
        return;
    };
    let replaced = files
        .iter_mut()
        .find(|file| previous.is_some_and(|previous| is_at(file, previous)));
    match replaced {
        Some(file) => file["path"] = json!(entrypoint),
        None => files
            .push(json!({ "path": entrypoint, "role": SCRIPT_ROLE, "mediaType": "text/plain" })),
    }
}
