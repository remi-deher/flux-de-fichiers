//! Création d'un flux enregistré depuis un modèle (liste des flux et écran de départ).

use super::{FLOWS_PATH, ID_SUFFIX_MODULO};
use crate::api::{self, ApiError};
use crate::route::encode_segment;
use forgearr_shared::flows::{FlowRecord, UpdateFlowRequest, new_flow_id};
use forgearr_shared::local::LocalFlowTemplate;

const EMPTY_DEFINITION: &str = "{}";

/// Requête d'enregistrement d'un flux qui reprend le graphe du modèle, sous ce nom et ces types de fichiers.
pub(super) fn request_from_template(
    template: &LocalFlowTemplate,
    name: &str,
    file_mask: &str,
) -> UpdateFlowRequest {
    UpdateFlowRequest {
        name: name.to_owned(),
        description: template.description.clone(),
        file_mask: file_mask.to_owned(),
        is_default: false,
        definition: serde_json::to_string(&template.definition)
            .unwrap_or_else(|_| EMPTY_DEFINITION.to_owned()),
    }
}

/// Crée le flux et renvoie son enregistrement.
pub(super) async fn create_flow_from_template(
    template: &LocalFlowTemplate,
    name: &str,
    file_mask: &str,
) -> Result<FlowRecord, ApiError> {
    let suffix = (js_sys::Date::now() as u64 % ID_SUFFIX_MODULO).to_string();
    let path = format!(
        "{FLOWS_PATH}/{}",
        encode_segment(&new_flow_id(name, &suffix))
    );
    api::put::<UpdateFlowRequest, FlowRecord>(
        &path,
        &request_from_template(template, name, file_mask),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template() -> LocalFlowTemplate {
        LocalFlowTemplate {
            id: "flow.remux".into(),
            name: "Conversion MKV".into(),
            description: Some("Remux sans perte".into()),
            file_mask: "*.mkv".into(),
            version: "1.0.0".into(),
            definition: json!({"nodes": [], "connections": []}),
            is_local: false,
        }
    }

    #[test]
    fn request_keeps_the_template_graph_and_takes_the_chosen_name_and_types() {
        let request = request_from_template(&template(), "Mes films", "*.mp4, *.mkv");
        assert_eq!(request.name, "Mes films");
        assert_eq!(request.file_mask, "*.mp4, *.mkv");
        assert_eq!(request.description.as_deref(), Some("Remux sans perte"));
        assert!(!request.is_default);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request.definition).ok(),
            Some(json!({"nodes": [], "connections": []}))
        );
    }
}
