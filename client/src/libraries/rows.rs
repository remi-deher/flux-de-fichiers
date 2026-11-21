//! Lignes éditables identifiées par une clé stable : éditeurs de variables et de dépendances d'un script.
//!
//! La clé ne change pas quand on tape dans une ligne : la liste n'est pas reconstruite et le champ garde le focus.

use leptos::prelude::*;

/// Ligne d'une liste éditable.
pub trait KeyedRow {
    fn key(&self) -> u32;
}

/// Lit une valeur de la ligne `key`, si elle existe encore.
pub fn read_row<R, T>(rows: RwSignal<Vec<R>>, key: u32, read: impl Fn(&R) -> T) -> Option<T>
where
    R: KeyedRow + Send + Sync + 'static,
{
    rows.with(|all| all.iter().find(|row| row.key() == key).map(read))
}

/// Modifie la ligne `key`, si elle existe encore.
pub fn edit_row<R>(rows: RwSignal<Vec<R>>, key: u32, edit: impl FnOnce(&mut R))
where
    R: KeyedRow + Send + Sync + 'static,
{
    rows.update(|all| {
        if let Some(row) = all.iter_mut().find(|row| row.key() == key) {
            edit(row);
        }
    });
}

/// Retire la ligne `key`.
pub fn remove_row<R>(rows: RwSignal<Vec<R>>, key: u32)
where
    R: KeyedRow + Send + Sync + 'static,
{
    rows.update(|all| all.retain(|row| row.key() != key));
}

/// Clés des lignes, dans l'ordre d'affichage.
pub fn row_keys<R>(rows: RwSignal<Vec<R>>) -> Vec<u32>
where
    R: KeyedRow + Send + Sync + 'static,
{
    rows.with(|all| all.iter().map(KeyedRow::key).collect())
}

/// Clé d'une nouvelle ligne : la suivante après la plus grande.
pub fn next_key<R: KeyedRow>(rows: &[R]) -> u32 {
    rows.iter()
        .map(KeyedRow::key)
        .max()
        .map_or(0, |last| last + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Row {
        key: u32,
        text: String,
    }

    impl KeyedRow for Row {
        fn key(&self) -> u32 {
            self.key
        }
    }

    fn row(key: u32, text: &str) -> Row {
        Row {
            key,
            text: text.into(),
        }
    }

    #[test]
    fn the_next_key_follows_the_largest_even_after_a_removal() {
        assert_eq!(next_key::<Row>(&[]), 0);
        assert_eq!(next_key(&[row(0, "a"), row(1, "b")]), 2);
        assert_eq!(next_key(&[row(0, "a"), row(5, "b")]), 6);
    }
}
