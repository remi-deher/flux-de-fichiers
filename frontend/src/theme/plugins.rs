//! Styles de la section Plugins et gestion multi-dépôts en Rust typé.

pub const PLUGINS_CSS: &str = r#"
/* --- Section Plugins et Outils distribués --- */
.tools-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 1.25rem;
  margin-top: 1rem;
}

.tool-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.875rem;
  box-shadow: var(--shadow-sm);
  transition: all 0.2s ease;
}

.tool-card:hover {
  border-color: var(--primary);
  box-shadow: var(--shadow-md);
}

.tool-card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.tool-name {
  font-size: 1.125rem;
  font-weight: 700;
  color: var(--text-main);
  margin: 0;
}

.tool-version-badge {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  padding: 0.2rem 0.5rem;
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  color: var(--text-dim);
}

.tool-desc {
  font-size: 0.875rem;
  color: var(--text-dim);
  line-height: 1.45;
  margin: 0;
}

.tool-card-footer {
  margin-top: auto;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-top: 1px solid var(--border-color);
  padding-top: 0.75rem;
}

.repo-badge {
  font-size: 0.6875rem;
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

/* --- Grille moderne du catalogue d'extensions --- */
.plugins-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(21rem, 1fr));
  gap: 1rem;
  margin-top: 0.5rem;
}

.plugin-card {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1.15rem 1.25rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  transition: border-color 0.16s ease, box-shadow 0.16s ease;
}

.plugin-card:hover {
  border-color: var(--border-highlight);
  box-shadow: var(--shadow-md);
}

.plugin-card__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.plugin-card__title-wrap {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-width: 0;
}

.plugin-card__icon {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  color: var(--primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.plugin-card__title {
  margin: 0;
  font-size: 1rem;
  font-weight: 700;
  color: var(--text-main);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.plugin-card__body {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  flex: 1;
}

.plugin-card__desc {
  margin: 0;
  font-size: 0.8125rem;
  color: var(--text-dim);
  line-height: 1.45;
}

.plugin-card__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.4rem;
}

.plugin-tag-chip {
  font-size: 0.7rem;
  font-weight: 600;
  padding: 0.15rem 0.45rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  color: var(--text-dim);
}

.plugin-card__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding-top: 0.75rem;
  margin-top: auto;
  border-top: 1px solid var(--border-subtle);
}
"#;
