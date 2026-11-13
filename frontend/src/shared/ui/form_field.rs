//! Champ de formulaire commun : étiquette, saisie et aide.

use leptos::prelude::*;

/// Longueur minimale d'un mot de passe, annoncée par le navigateur (le serveur la vérifie aussi).
pub const PASSWORD_MIN_LENGTH: u32 = 8;

#[component]
pub fn FormField(
    id: &'static str,
    label: &'static str,
    #[prop(default = "text")] input_type: &'static str,
    #[prop(optional)] autocomplete: Option<&'static str>,
    #[prop(optional)] help: Option<&'static str>,
    #[prop(optional)] required: bool,
    #[prop(optional)] minlength: Option<u32>,
    #[prop(optional)] placeholder: Option<&'static str>,
) -> impl IntoView {
    view! {
        <div class="form-group">
            <label for=id class="form-label">{label}</label>
            <input
                id=id
                name=id
                class="form-input"
                type=input_type
                autocomplete=autocomplete
                required=required
                minlength=minlength
                placeholder=placeholder
                spellcheck="false"
            />
            {help.map(|text| view! { <p class="form-help">{text}</p> })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_links_label_and_input_and_defaults_to_text() {
        let html = view! { <FormField id="champ" label="Nom" /> }.to_html();
        assert!(html.contains("for=\"champ\""));
        assert!(html.contains("id=\"champ\""));
        assert!(html.contains("type=\"text\""));
        assert!(!html.contains("form-help"));
    }

    #[test]
    fn test_field_renders_type_constraints_and_help() {
        let html = view! {
            <FormField id="secret" label="Mot de passe" input_type="password" autocomplete="new-password" required=true minlength=8 help="Au moins 8 caractères." />
        }
        .to_html();
        assert!(html.contains("type=\"password\""));
        assert!(html.contains("autocomplete=\"new-password\""));
        assert!(html.contains("minlength=\"8\""));
        assert!(html.contains("Au moins 8 caractères."));
    }
}
