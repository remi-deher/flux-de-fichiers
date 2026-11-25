//! Lecture et enregistrement d'un script de la collection, de l'utilisateur ou installé.

use super::{
    MAX_SCRIPT_BYTES, SCRIPT_ID_PREFIX, has_manifest, item_dir, normalize_description,
    operation_id, rebuild_local_index, resolve_slug, script_dir, script_documents::ScriptDocuments,
    script_item_dir, script_slug,
};
use crate::{
    error::{AppError, AppResult},
    flows::OperationRegistry,
    plugins::CatalogEntry,
};
use forgearr_shared::local::{LocalScriptSummary, UpsertLocalScriptRequest};
use std::path::Path;

/// Lit le manifeste, l'opération et le code d'un script du catalogue situé sous `root`.
pub(in crate::plugins) fn read_local_script(
    root: &Path,
    entry: &CatalogEntry,
) -> Option<LocalScriptSummary> {
    let dir = item_dir(root, entry).ok()?;
    let documents = ScriptDocuments::load(&dir).ok()??;
    Some(LocalScriptSummary {
        id: entry.id.clone(),
        slug: script_slug(&entry.id).to_owned(),
        name: entry.name.clone(),
        description: documents.description(),
        interpreter: documents.interpreter()?,
        entrypoint: documents.entrypoint()?,
        code: documents.code(),
        category: entry.category,
        tags: entry.tags.clone(),
        variables: documents.runtime_field("variables").unwrap_or_default(),
        requires: documents.runtime_field("requires").unwrap_or_default(),
        dependencies_unverified: documents
            .runtime_field("dependenciesUnverified")
            .unwrap_or_default(),
    })
}

/// Réécrit au format `{$…}` les défauts des variables qui citent encore l'ancienne syntaxe
/// `{{…}}` (ADR 0026) ; un secret n'a jamais de défaut et n'est pas concerné.
fn upgrade_legacy_defaults(variables: &mut [forgearr_shared::script_variables::ScriptVariable]) {
    for default in variables
        .iter_mut()
        .filter_map(|variable| variable.default_value.as_mut())
    {
        *default = forgearr_shared::flows::legacy_references::upgrade_legacy_strings(
            default,
            forgearr_shared::flows::legacy_references::LegacyConversion::AllPaths,
        );
    }
}

fn script_id(slug: &str) -> String {
    format!("{SCRIPT_ID_PREFIX}{slug}")
}

/// Un script du même dossier, ou installé avec la même opération, empêche-t-il d'en créer un sous ce nom ?
fn is_slug_taken(local_dir: &Path, slug: &str) -> bool {
    has_manifest(&script_dir(local_dir, slug))
        || script_item_dir(local_dir, &script_id(slug)).is_ok_and(|dir| dir.is_some())
        || OperationRegistry::load_from_roots(&[local_dir.to_path_buf()])
            .get(&operation_id(slug))
            .is_some()
}

/// Créer un script de ce nom serait-il refusé, un autre l'occupant déjà ?
pub(in crate::plugins) fn script_name_taken(local_dir: &Path, name: &str) -> bool {
    resolve_slug(None, SCRIPT_ID_PREFIX, name)
        .is_ok_and(|(slug, _)| is_slug_taken(local_dir, &slug))
}

/// Documents du script visé, ou d'un nouveau script.
fn target_documents(
    local_dir: &Path,
    req: &UpsertLocalScriptRequest,
) -> AppResult<ScriptDocuments> {
    let id = req.id.as_deref().map(str::trim).filter(|id| !id.is_empty());
    if let Some(id) = id
        && let Some(dir) = script_item_dir(local_dir, id)?
        && let Some(known) = ScriptDocuments::load(&dir)?
    {
        return Ok(known);
    }
    let (slug, _) = resolve_slug(id, SCRIPT_ID_PREFIX, req.name.trim())?;
    if is_slug_taken(local_dir, &slug) {
        return Err(AppError::Conflict);
    }
    Ok(ScriptDocuments::new(
        script_dir(local_dir, &slug),
        script_id(&slug),
    ))
}

/// Crée un script (opération et bloc compris), ou reporte la saisie dans un script de la collection.
pub fn save_local_script(
    local_dir: &Path,
    mut req: UpsertLocalScriptRequest,
) -> AppResult<LocalScriptSummary> {
    upgrade_legacy_defaults(&mut req.variables);
    if req.name.trim().is_empty() {
        return Err(AppError::InvalidManifest(
            "le nom du script est requis".into(),
        ));
    }
    if req.code.len() > MAX_SCRIPT_BYTES {
        return Err(AppError::TooLarge);
    }
    forgearr_shared::script_variables::validate_variables(&req.variables)
        .map_err(|err| AppError::InvalidManifest(err.to_string()))?;
    forgearr_shared::exposed_variables::validate_requirements(&req.requires)
        .map_err(|err| AppError::InvalidManifest(err.to_string()))?;
    let mut documents = target_documents(local_dir, &req)?;
    documents.apply(&req)?;
    documents.write(&req.code)?;
    rebuild_local_index(local_dir)?;
    Ok(LocalScriptSummary {
        id: documents.id().to_owned(),
        slug: script_slug(documents.id()).to_owned(),
        name: req.name.trim().to_owned(),
        description: normalize_description(req.description.as_deref()),
        interpreter: req.interpreter,
        entrypoint: documents.entrypoint().unwrap_or_default(),
        code: req.code,
        category: req.category,
        tags: req.tags,
        variables: req.variables,
        requires: req.requires,
        dependencies_unverified: req.dependencies_unverified,
    })
}
