//! Prompts MCP prêts à l'emploi pour guider les assistants externes (ADR 0015).

use super::protocol::{McpPrompt, McpPromptArgument, McpPromptMessage, McpPromptText};
use serde_json::Value;

const DIAGNOSE_ARGS: &[McpPromptArgument] = &[McpPromptArgument {
    name: "jobId",
    description: "UUID d'une tâche spécifique à analyser (optionnel ; sans UUID, examine les derniers échecs).",
    required: false,
}];

const DESIGN_FLOW_ARGS: &[McpPromptArgument] = &[
    McpPromptArgument {
        name: "goal",
        description: "Objectif de traitement attendu (ex. encoder en AV1 matériel avec sous-titres FR).",
        required: true,
    },
    McpPromptArgument {
        name: "libraryId",
        description: "UUID d'une bibliothèque cible optionnelle pour adapter les règles.",
        required: false,
    },
];

const AUDIT_LIBRARIES_ARGS: &[McpPromptArgument] = &[];

pub fn list() -> Vec<McpPrompt> {
    vec![
        McpPrompt {
            name: "diagnose_failures",
            description: "Diagnostique les tâches en échec grâce au rapport agrégé et au matériel disponible.",
            arguments: DIAGNOSE_ARGS,
        },
        McpPrompt {
            name: "design_flow",
            description: "Conçoit un graphe de flux valide et le teste à sec avec `simulate_flow` avant sauvegarde.",
            arguments: DESIGN_FLOW_ARGS,
        },
        McpPrompt {
            name: "audit_libraries",
            description: "Analyse les économies disque par bibliothèque et recommande des règles d'automatisation.",
            arguments: AUDIT_LIBRARIES_ARGS,
        },
    ]
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub fn get(name: &str, args: &Value) -> Result<(&'static str, Vec<McpPromptMessage>), String> {
    match name {
        "diagnose_failures" => {
            let instruction = match arg_str(args, "jobId") {
                Some(job_id) => format!(
                    "Analyse l'échec de la tâche `{job_id}` :\n\
                     1. Appelle `diagnose_job` avec `id: \"{job_id}\"` pour récupérer l'état, `job_steps`, la version figée du flux et le worker.\n\
                     2. Appelle `get_system_hardware` et `list_workers` si besoin pour vérifier GPU/RAM.\n\
                     3. Explique la cause racine en français, propose un correctif sur le flux ou les réglages, puis demande confirmation avant toute modification."
                ),
                None => "Analyse les derniers échecs du central :\n\
                         1. Appelle `list_jobs` puis sélectionne les tâches `failed` ou `canceled`.\n\
                         2. Appelle `diagnose_job` sur les échecs pertinents et `get_job_stats` pour la vue globale.\n\
                         3. Résume les causes récurrentes et propose des corrections concrètes."
                    .to_owned(),
            };
            Ok((
                "Diagnostic structuré des tâches en échec",
                vec![McpPromptMessage {
                    role: "user",
                    content: McpPromptText {
                        kind: "text",
                        text: instruction,
                    },
                }],
            ))
        }
        "design_flow" => {
            let goal = arg_str(args, "goal")
                .ok_or_else(|| "argument requis manquant ou vide : goal".to_owned())?;
            let lib_note = arg_str(args, "libraryId")
                .map(|id| format!(" pour la bibliothèque `{id}` (lis-la via `get_library`)"))
                .unwrap_or_default();
            let text = format!(
                "Conçois un flux Flux de Fichiers pour l'objectif suivant{lib_note} : {goal}\n\
                 1. Appelle `list_blocks` pour connaître exactement les types de blocs et champs disponibles.\n\
                 2. Appelle `get_system_hardware` et `list_workers` pour adapter l'encodeur matériel (VAAPI/NVENC/CPU).\n\
                 3. Construis le graphe JSON complet, valide-le avec `validate_flow`, puis simule-le avec `simulate_flow` sur les profils utiles (`structured_tracks`, `minimal`).\n\
                 4. Enregistre le flux avec `save_flow` uniquement une fois la simulation complète."
            );
            Ok((
                "Conception et validation d'un nouveau flux",
                vec![McpPromptMessage {
                    role: "user",
                    content: McpPromptText { kind: "text", text },
                }],
            ))
        }
        "audit_libraries" => Ok((
            "Audit des bibliothèques et gains d'espace disque",
            vec![McpPromptMessage {
                role: "user",
                content: McpPromptText {
                    kind: "text",
                    text: "Audite les bibliothèques du central :\n\
                           1. Appelle `get_job_stats`, `list_libraries` et `list_flows`.\n\
                           2. Identifie les gains ou surcoûts de volume, les bibliothèques sans flux ou sans scan planifié, et les extensions/exclusions manquantes.\n\
                           3. Propose des améliorations chiffrées avant d'appeler `update_library`."
                        .to_owned(),
                },
            }],
        )),
        other => Err(format!("prompt inconnu : {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prompts_list_and_get_validate_arguments() {
        assert_eq!(list().len(), 3);
        assert!(get("diagnose_failures", &json!({})).is_ok());
        assert!(get("diagnose_failures", &json!({ "jobId": "abc" })).is_ok());
        assert!(get("design_flow", &json!({})).is_err());
        assert!(get("design_flow", &json!({ "goal": "AV1" })).is_ok());
        assert!(get("audit_libraries", &json!({})).is_ok());
        assert!(get("unknown", &json!({})).is_err());
    }
}
