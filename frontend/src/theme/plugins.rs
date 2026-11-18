//! Styles de la section Dépôts de plugins en Rust typé (grilles portées par `components.rs`).

pub const PLUGINS_CSS: &str = r#"
/* --- Étiquettes de statut de dépôt --- */
.repo-badge {
  font-size: var(--font-size-xs);
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 0.15rem 0.45rem;
  border-radius: var(--radius-sm);
}

.repo-badge.official {
  background: var(--primary-bg);
  color: var(--primary);
  border: 1px solid var(--primary);
}

.repo-badge.community {
  background: var(--bg-subtle);
  color: var(--text-dim);
  border: 1px solid var(--border-color);
}
"#;
