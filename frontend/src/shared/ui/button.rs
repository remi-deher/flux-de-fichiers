//! Composant Bouton atomique réutilisable conforme aux standards tactiles W3C et Google/Apple.

use leptos::prelude::*;

/// Variante visuelle du bouton basée sur les Design Tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
}

impl ButtonVariant {
    pub const fn as_class(self) -> &'static str {
        match self {
            Self::Primary => "btn btn-primary",
            Self::Secondary => "btn btn-secondary",
            Self::Danger => "btn btn-danger",
        }
    }
}

/// Composant Bouton atomique réutilisable sans boilerplate.
#[component]
pub fn Button(
    #[prop(optional, default = ButtonVariant::default())] variant: ButtonVariant,
    #[prop(optional)] id: Option<&'static str>,
    #[prop(optional)] class: Option<&'static str>,
    #[prop(optional)] aria_label: Option<&'static str>,
    #[prop(optional)] disabled: bool,
    children: Children,
) -> impl IntoView {
    let base_class = variant.as_class();
    let final_class = match class {
        Some(extra) => format!("{base_class} {extra}"),
        None => base_class.to_string(),
    };

    view! {
        <button
            type="button"
            id=id
            class=final_class
            aria-label=aria_label
            disabled=disabled
        >
            {children()}
        </button>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_variant_classes() {
        assert_eq!(ButtonVariant::Primary.as_class(), "btn btn-primary");
        assert_eq!(ButtonVariant::Secondary.as_class(), "btn btn-secondary");
        assert_eq!(ButtonVariant::Danger.as_class(), "btn btn-danger");
    }
}
