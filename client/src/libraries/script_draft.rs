//! Brouillon d'un script local : état ouvert, comparaison avec la librairie relue et gabarits (ADR 0030).

use crate::live::ChangedElsewhere;
use forgearr_shared::exposed_variables::ScriptRequirement;
use forgearr_shared::local::{LocalScriptSummary, ScriptInterpreter};
use forgearr_shared::plugins::vocab::PluginCategory;
use forgearr_shared::script_variables::ScriptVariable;
use leptos::prelude::*;

pub(super) const CATEGORIES: &[(Option<PluginCategory>, &str)] = &[
    (None, "Standard (aucun badge)"),
    (Some(PluginCategory::Media), "Média"),
    (Some(PluginCategory::Data), "Données"),
    (Some(PluginCategory::Io), "Entrées / Sorties"),
];

pub fn starter_script_code(interpreter: ScriptInterpreter) -> &'static str {
    match interpreter {
        ScriptInterpreter::Python3 => {
            "import json, sys\n\npayload = json.load(sys.stdin)\nprint(json.dumps({\"status\": \"ok\", \"message\": \"Script Python exécuté\"}))\n"
        }
        ScriptInterpreter::Node | ScriptInterpreter::Bun => {
            "const fs = require('fs');\nconst input = JSON.parse(fs.readFileSync(0, 'utf8'));\nconsole.log(JSON.stringify({ status: 'ok', message: 'Script JS exécuté' }));\n"
        }
        ScriptInterpreter::Deno => {
            "const text = await new Response(Deno.stdin.readable).text();\nconst input = text.trim() ? JSON.parse(text) : {};\nconsole.log(JSON.stringify({ status: 'ok', message: 'Script Deno exécuté' }));\n"
        }
        ScriptInterpreter::Pwsh => {
            "$payload = [Console]::In.ReadToEnd() | ConvertFrom-Json\n@{ status = 'ok'; message = 'Script PowerShell exécuté' } | ConvertTo-Json -Compress\n"
        }
        ScriptInterpreter::Sh | ScriptInterpreter::Bash => {
            "cat >/dev/null\nprintf '{\"status\":\"ok\",\"message\":\"Script exécuté\"}\\n'\n"
        }
    }
}

pub fn is_starter_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty()
        || ScriptInterpreter::ALL
            .iter()
            .any(|&interp| starter_script_code(interp).trim() == trimmed)
}

pub fn parse_tags(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in raw.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !out.iter().any(|existing: &String| existing == tag) {
            out.push(tag.to_owned());
        }
    }
    out
}

pub(super) fn category_value(cat: Option<PluginCategory>) -> &'static str {
    match cat {
        Some(c) if c.shows_badge() => c.as_str(),
        _ => "",
    }
}

pub(super) fn parse_category(raw: &str) -> Option<PluginCategory> {
    match raw.trim() {
        "media" => Some(PluginCategory::Media),
        "data" => Some(PluginCategory::Data),
        "io" => Some(PluginCategory::Io),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ScriptDraft {
    pub(super) id: Option<String>,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) interpreter: ScriptInterpreter,
    pub(super) filename: String,
    pub(super) code: String,
    pub(super) category: Option<PluginCategory>,
    pub(super) tags: String,
    pub(super) variables: Vec<ScriptVariable>,
    pub(super) requires: Vec<ScriptRequirement>,
    pub(super) dependencies_unverified: bool,
}

impl ScriptDraft {
    pub(super) fn blank() -> Self {
        let interp = ScriptInterpreter::Sh;
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            interpreter: interp,
            filename: format!("run.{}", interp.default_extension()),
            code: starter_script_code(interp).to_owned(),
            category: None,
            tags: String::new(),
            variables: Vec::new(),
            requires: Vec::new(),
            dependencies_unverified: false,
        }
    }

    pub(super) fn from_summary(s: &LocalScriptSummary) -> Self {
        Self {
            id: Some(s.id.clone()),
            name: s.name.clone(),
            description: s.description.clone().unwrap_or_default(),
            interpreter: s.interpreter,
            filename: s.entrypoint.clone(),
            code: s.code.clone(),
            category: s.category.filter(|c| c.shows_badge()),
            tags: s.tags.join(", "),
            variables: s.variables.clone(),
            requires: s.requires.clone(),
            dependencies_unverified: s.dependencies_unverified,
        }
    }
}

/// Ce que le script ouvert est devenu dans la librairie relue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OpenedScript {
    Unchanged,
    Changed(Box<ScriptDraft>),
    Removed,
}

impl OpenedScript {
    /// `opened` est l'état à l'ouverture, sans la saisie ; un nouveau script n'a rien à comparer.
    pub(super) fn of(opened: &ScriptDraft, scripts: &[LocalScriptSummary]) -> Self {
        let Some(id) = opened.id.as_deref() else {
            return Self::Unchanged;
        };
        match scripts.iter().find(|script| script.id == id) {
            None => Self::Removed,
            Some(script) => {
                let current = ScriptDraft::from_summary(script);
                if current == *opened {
                    Self::Unchanged
                } else {
                    Self::Changed(Box::new(current))
                }
            }
        }
    }
}

/// Avertissement affiché au-dessus du script ouvert ; `on_reload` remplace la saisie.
pub(super) fn opened_script_warning(
    state: OpenedScript,
    on_reload: Callback<ScriptDraft>,
) -> Option<AnyView> {
    match state {
        OpenedScript::Unchanged => None,
        OpenedScript::Changed(current) => {
            let reload = Callback::new(move |()| on_reload.run((*current).clone()));
            Some(
                view! {
                    <ChangedElsewhere
                        text="Ce script a changé depuis son ouverture. Recharger remplace votre saisie ; enregistrer remplace ces changements."
                        on_reload=Some(reload)
                    />
                }
                .into_any(),
            )
        }
        OpenedScript::Removed => {
            Some(view! { <ChangedElsewhere text="Ce script a été supprimé." /> }.into_any())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_script_code_provides_valid_templates_per_runtime() {
        for &interp in ScriptInterpreter::ALL {
            let starter = starter_script_code(interp);
            assert!(starter.contains("status"));
            assert!(is_starter_code(starter));
        }
        assert!(starter_script_code(ScriptInterpreter::Python3).contains("sys.stdin"));
        assert!(starter_script_code(ScriptInterpreter::Node).contains("readFileSync"));
        assert!(starter_script_code(ScriptInterpreter::Deno).contains("Deno.stdin"));
        assert!(starter_script_code(ScriptInterpreter::Pwsh).contains("ConvertFrom-Json"));
        assert!(!is_starter_code("echo custom script"));
    }

    #[test]
    fn an_opened_script_notices_changes_and_removal() -> Result<(), serde_json::Error> {
        let script: LocalScriptSummary = serde_json::from_value(serde_json::json!({
            "id": "script.x", "slug": "x", "name": "X", "interpreter": ScriptInterpreter::Sh,
            "entrypoint": "run.sh", "code": "true"
        }))?;
        let opened = ScriptDraft::from_summary(&script);
        let edited = LocalScriptSummary {
            code: "false".into(),
            ..script.clone()
        };
        assert_eq!(
            OpenedScript::of(&opened, std::slice::from_ref(&script)),
            OpenedScript::Unchanged
        );
        assert_eq!(
            OpenedScript::of(&opened, std::slice::from_ref(&edited)),
            OpenedScript::Changed(Box::new(ScriptDraft::from_summary(&edited)))
        );
        assert_eq!(OpenedScript::of(&opened, &[]), OpenedScript::Removed);
        assert_eq!(
            OpenedScript::of(&ScriptDraft::blank(), &[]),
            OpenedScript::Unchanged
        );
        Ok(())
    }

    #[test]
    fn parse_tags_trims_and_deduplicates() {
        assert_eq!(
            parse_tags(" mkv, audio , , mkv , hdr "),
            vec!["mkv", "audio", "hdr"]
        );
        assert!(parse_tags("   ").is_empty());
    }
}
