//! Protocole des scripts (`kind: "script"`) : JSON sur `stdin`, réponse JSON sur `stdout`.
//!
//! Le moteur et l'inspecteur de l'éditeur lisent ces mêmes constantes : la documentation affichée
//! ne peut pas diverger des champs réellement envoyés et lus.

/// Version du protocole ; 1.1 ajoute les variables de flux (changement additif).
pub const SCRIPT_SCHEMA_VERSION: &str = "1.1";

/// Champ du protocole et sa description.
pub type ProtocolField = (&'static str, &'static str);

/// Champs de la charge JSON envoyée sur `stdin`.
pub const SCRIPT_INPUT_FIELDS: [ProtocolField; 10] = [
    ("schemaVersion", "version du protocole"),
    ("flowId", "identifiant du flux"),
    ("nodeId", "identifiant du bloc"),
    ("operationId", "contrat d'opération exécuté"),
    ("workingFile", "fichier de travail courant"),
    ("sourceFile", "fichier d'origine de la tâche"),
    ("workDir", "dossier de travail (seul dossier accepté en sortie)"),
    ("mediaInfo", "métadonnées de la sonde"),
    ("config", "réglages du bloc, modèles {$…} déjà rendus"),
    ("variables", "variables du flux : file.*, job.*, library.*, worker.*, vars.*"),
];

/// Champs reconnus dans la réponse JSON lue sur `stdout` (tous facultatifs).
pub const SCRIPT_OUTPUT_FIELDS: [ProtocolField; 7] = [
    ("status", "ok, skip ou error"),
    ("message", "message affiché dans le journal de la tâche"),
    ("nextPort", "sortie à emprunter en routage conditionnel"),
    ("workingFile", "nouveau fichier de travail, dans workDir"),
    ("mediaInfo", "métadonnées fusionnées dans la sonde"),
    ("stopPipeline", "arrête le flux après ce bloc"),
    ("variables", "objet plat nom → texte, nombre ou booléen, fusionné dans vars.*"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_fields_are_unique_and_documented() {
        for fields in [&SCRIPT_INPUT_FIELDS[..], &SCRIPT_OUTPUT_FIELDS[..]] {
            for (index, (name, description)) in fields.iter().enumerate() {
                assert!(!description.is_empty());
                assert!(fields[index + 1..].iter().all(|(other, _)| other != name));
            }
        }
    }
}
