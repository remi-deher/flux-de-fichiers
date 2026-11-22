//! État du formulaire de réglages : saisie en cours, confrontée au contenu relu du central.

use forgearr_shared::plugins::{PluginSettingsView, ResolvedSettingsField, SettingsFieldType};

/// Que faire d'un contenu relu pendant que le formulaire est ouvert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Incoming {
    /// Rien de neuf.
    Keep,
    /// Le formulaire n'est pas modifié, ou porte déjà ce contenu.
    Adopt,
    /// Contenu différent et saisie en cours : prévenir sans rien écraser.
    Warn,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsForm {
    /// Dernier contenu du central pris en compte.
    origin: Option<PluginSettingsView>,
    /// Champs de référence : contenu du central, ou dernier enregistrement de ce formulaire.
    fields: Vec<ResolvedSettingsField>,
    drafts: Vec<String>,
}

impl SettingsForm {
    /// Champs à afficher, sans leur état : une valeur qui change ne redessine pas le formulaire.
    /// `None` avant la première lecture.
    pub fn shape(&self) -> Option<Vec<ResolvedSettingsField>> {
        self.origin.as_ref().map(|_| {
            let stateless = |field: &ResolvedSettingsField| ResolvedSettingsField {
                value: String::new(),
                is_set: false,
                ..field.clone()
            };
            self.fields.iter().map(stateless).collect()
        })
    }

    pub fn is_set(&self, index: usize) -> bool {
        self.fields.get(index).is_some_and(|field| field.is_set)
    }

    pub fn draft(&self, index: usize) -> String {
        self.drafts.get(index).cloned().unwrap_or_default()
    }

    pub fn type_in(&mut self, index: usize, value: String) {
        if let Some(draft) = self.drafts.get_mut(index) {
            *draft = value;
        }
    }

    /// Champs et valeurs saisies, à enregistrer ou à tester.
    pub fn entries(&self) -> Vec<(ResolvedSettingsField, String)> {
        self.fields
            .iter()
            .cloned()
            .zip(self.drafts.iter().cloned())
            .collect()
    }

    fn is_dirty(&self) -> bool {
        self.fields
            .iter()
            .zip(&self.drafts)
            .any(|(field, draft)| field.value != *draft)
    }

    pub fn incoming(&self, fresh: &PluginSettingsView) -> Incoming {
        if self.origin.as_ref() == Some(fresh) {
            Incoming::Keep
        } else if !self.is_dirty() || self.fields == fresh.fields {
            Incoming::Adopt
        } else {
            Incoming::Warn
        }
    }

    /// Prend le contenu du central ; la saisie n'est gardée que si elle part déjà de ce contenu.
    pub fn adopt(&mut self, fresh: PluginSettingsView) {
        if !(self.is_dirty() && self.fields == fresh.fields) {
            self.reset(fresh);
        } else {
            self.origin = Some(fresh);
        }
    }

    /// Remplace la saisie par le contenu du central.
    pub fn reset(&mut self, fresh: PluginSettingsView) {
        self.drafts = fresh
            .fields
            .iter()
            .map(|field| field.value.clone())
            .collect();
        self.fields = fresh.fields.clone();
        self.origin = Some(fresh);
    }

    /// Le central a accepté `submitted` : ce qui a été saisi depuis reste une modification.
    /// `seen`, dernier contenu lu à l'envoi, vient d'être remplacé : il ne sera pas repris.
    pub fn saved(
        &mut self,
        submitted: &[(ResolvedSettingsField, String)],
        seen: Option<PluginSettingsView>,
    ) {
        if seen.is_some() {
            self.origin = seen;
        }
        for (field, draft) in self.fields.iter_mut().zip(&mut self.drafts) {
            let Some((_, sent)) = submitted.iter().find(|(sent, _)| sent.name == field.name) else {
                continue;
            };
            if field.field_type == SettingsFieldType::Secret {
                field.is_set |= !sent.is_empty();
                if draft == sent {
                    draft.clear();
                }
            } else {
                field.value.clone_from(sent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, kind: SettingsFieldType, value: &str) -> ResolvedSettingsField {
        ResolvedSettingsField {
            name: name.into(),
            label: name.into(),
            field_type: kind,
            required: false,
            description: None,
            value: value.into(),
            is_set: false,
        }
    }

    fn view(url: &str) -> PluginSettingsView {
        PluginSettingsView {
            plugin_id: "p".into(),
            settings: serde_json::Map::new(),
            fields: vec![
                field("url", SettingsFieldType::Url, url),
                field("key", SettingsFieldType::Secret, ""),
            ],
        }
    }

    fn opened(url: &str) -> SettingsForm {
        let mut form = SettingsForm::default();
        assert_eq!(form.incoming(&view(url)), Incoming::Adopt);
        form.adopt(view(url));
        form
    }

    #[test]
    fn an_untouched_form_follows_the_central() {
        let mut form = opened("a");
        assert_eq!(form.incoming(&view("a")), Incoming::Keep);
        assert_eq!(form.incoming(&view("b")), Incoming::Adopt);
        form.adopt(view("b"));
        assert_eq!(form.draft(0), "b");
    }

    #[test]
    fn a_draft_is_never_overwritten_without_reload() {
        let mut form = opened("a");
        form.type_in(0, "saisie".into());
        assert_eq!(form.incoming(&view("b")), Incoming::Warn);
        assert_eq!(form.draft(0), "saisie");
        form.reset(view("b"));
        assert_eq!(form.draft(0), "b");
        assert_eq!(form.incoming(&view("b")), Incoming::Keep);
    }

    #[test]
    fn a_save_despite_the_warning_never_brings_back_what_it_replaced() {
        let mut form = opened("a");
        form.type_in(0, "b".into());
        assert_eq!(form.incoming(&view("c")), Incoming::Warn);
        let submitted = form.entries();
        form.saved(&submitted, Some(view("c")));
        assert_eq!(form.incoming(&view("c")), Incoming::Keep);
        assert_eq!(form.draft(0), "b");
        // La relecture a pu arriver avant la réponse : elle est adoptée sans rien changer.
        assert_eq!(form.incoming(&view("b")), Incoming::Adopt);
        form.adopt(view("b"));
        assert_eq!(form.draft(0), "b");
    }

    #[test]
    fn values_never_change_the_shape() {
        let mut form = opened("a");
        let shape = form.shape();
        form.type_in(0, "b".into());
        form.type_in(1, "secret".into());
        let submitted = form.entries();
        form.saved(&submitted, None);
        assert!(form.is_set(1));
        assert_eq!(form.shape(), shape);
        form.adopt(view("c"));
        assert_eq!(form.draft(0), "c");
        assert!(!form.is_set(1));
        assert_eq!(form.shape(), shape);
    }

    #[test]
    fn its_own_save_is_not_a_foreign_change() {
        let mut form = opened("a");
        form.type_in(0, "b".into());
        form.type_in(1, "secret".into());
        let submitted = form.entries();
        form.saved(&submitted, Some(view("a")));
        assert_eq!(form.draft(1), "");
        // La relecture d'avant l'enregistrement ne revient pas sur la saisie.
        assert_eq!(form.incoming(&view("a")), Incoming::Keep);
        let mut fresh = view("b");
        fresh.fields[1].is_set = true;
        assert_eq!(form.incoming(&fresh), Incoming::Adopt);
        form.type_in(0, "suite".into());
        assert_eq!(form.incoming(&fresh), Incoming::Adopt);
        form.adopt(fresh);
        assert_eq!(form.draft(0), "suite");
    }
}
