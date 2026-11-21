//! Styles de la section Plugins (style App Store, vitrine Packs, bandeaux d'état et tiroirs) en Rust typé.

pub const PLUGINS_CSS: &str = r#"
/* --- Barre de filtres et contrôles de la vue Catalogue --- */
.store-toolbar {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 0.75rem;
  margin-bottom: 1.25rem;
}

.store-toolbar > .stat-strip {
  width: 100%;
  margin-bottom: 0;
}

.store-toolbar__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 0.75rem;
}

.page-subnav__tab {
  gap: 0.45rem;
}

.store-segmented {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.2rem;
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-pill);
}

.store-segmented__btn {
  appearance: none;
  border: 0;
  background: transparent;
  color: var(--text-dim);
  font-size: var(--font-size-xs);
  font-weight: 600;
  padding: 0.35rem 0.8rem;
  border-radius: var(--radius-pill);
  cursor: pointer;
  transition: background 140ms ease, color 140ms ease;
}

.store-segmented__btn:hover {
  color: var(--text-main);
}

.store-segmented__btn.is-active {
  background: var(--primary-bg);
  color: var(--primary);
}

/* --- En-têtes de section du Store --- */
.store-section {
  margin-bottom: 1.75rem;
}

.store-section + .store-section {
  padding-top: 1.5rem;
  border-top: 1px solid var(--border-subtle);
}

.store-section__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.75rem;
  margin-bottom: 0.85rem;
}

.store-section__title {
  margin: 0;
  font-size: var(--font-size-lg);
  font-weight: 700;
  color: var(--text-main);
  letter-spacing: -0.01em;
}

.store-section__subtitle {
  margin: 0;
  font-size: var(--font-size-xs);
  color: var(--text-dim);
}

/* --- Vitrine Packs recommandés --- */
.featured-packs {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(19rem, 1fr));
  gap: 1rem;
}

.featured-pack {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1.15rem 1.25rem;
  background: linear-gradient(155deg, var(--primary-bg) 0%, var(--bg-card) 58%);
  border: 1px solid var(--border-highlight);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  cursor: pointer;
  transition: transform 150ms ease, border-color 150ms ease, box-shadow 150ms ease;
}

.featured-pack:hover,
.featured-pack.is-open {
  transform: translateY(-2px);
  border-color: var(--primary);
  box-shadow: var(--shadow-md);
}

.featured-pack:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}

.featured-pack__top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
}

.featured-pack__icon {
  width: 2.5rem;
  height: 2.5rem;
  border-radius: var(--radius-md);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--primary-bg);
  color: var(--primary);
  border: 1px solid var(--border-highlight);
  flex-shrink: 0;
}

.featured-pack__title {
  margin: 0;
  font-size: var(--font-size-lg);
  font-weight: 700;
  color: var(--text-main);
}

.featured-pack__desc {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--text-dim);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.featured-pack__foot {
  margin-top: auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-subtle);
}

/* --- Grille App Store responsive --- */
.store-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
}

.store-card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1.05rem 1.15rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  cursor: pointer;
  transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
}

.store-card:hover,
.store-card.is-open {
  transform: translateY(-2px);
  border-color: var(--primary);
  box-shadow: var(--shadow-md);
}

.store-card:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}

.store-card__head {
  display: flex;
  align-items: flex-start;
  gap: 0.85rem;
}

.store-card__icon {
  width: 2.4rem;
  height: 2.4rem;
  border-radius: var(--radius-md);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-subtle);
  color: var(--text-main);
  border: 1px solid var(--border-subtle);
  flex-shrink: 0;
}

/* Icône SVG d'un plugin : masque d'image qui reprend la couleur du texte (ADR 0024). */
.plugin-icon {
  display: inline-block;
  width: 1.25rem;
  height: 1.25rem;
  background-color: currentColor;
  -webkit-mask: var(--plugin-icon-url) center / contain no-repeat;
  mask: var(--plugin-icon-url) center / contain no-repeat;
}

.store-card[data-tone="success"] .store-card__icon {
  background: var(--success-bg);
  color: var(--success);
}

.store-card[data-tone="warning"] .store-card__icon {
  background: var(--warning-bg);
  color: var(--warning);
}

.store-card[data-tone="primary"] .store-card__icon {
  background: var(--primary-bg);
  color: var(--primary);
}

.store-card__identity {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.store-card__title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.store-card__title {
  margin: 0;
  font-size: var(--font-size-md);
  font-weight: 700;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.store-card__desc {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--text-dim);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.store-card__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.4rem;
}

.store-card__foot {
  margin-top: auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding-top: 0.7rem;
  border-top: 1px solid var(--border-subtle);
}

.store-card__version {
  font-size: var(--font-size-xs);
  color: var(--text-dim);
  font-family: var(--font-mono);
}

.store-card__actions {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
}

/* --- Cartes Dépôts --- */
.repo-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(22rem, 1fr));
  gap: 1rem;
}

.repo-card__url {
  margin: 0;
  font-family: var(--font-mono);
  font-size: var(--font-size-xs);
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* --- Bandeau d'état dans les tiroirs et formulaires --- */
.status-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.85rem;
  padding: 0.85rem 1rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-elevated);
  margin-bottom: 1rem;
}

.status-banner[data-tone="success"] {
  background: var(--success-bg);
  border-color: var(--success);
}

.status-banner[data-tone="warning"] {
  background: var(--warning-bg);
  border-color: var(--warning);
}

.status-banner[data-tone="danger"] {
  background: var(--danger-bg);
  border-color: var(--danger);
}

.status-banner__content {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  min-width: 0;
}

.status-banner__title {
  margin: 0;
  font-size: var(--font-size-sm);
  font-weight: 700;
  color: var(--text-main);
}

.status-banner__text {
  margin: 0;
  font-size: var(--font-size-xs);
  color: var(--text-dim);
}

/* --- Liens croisés cliquables dans les tiroirs (dépendances <-> plugins) --- */
.dep-link {
  width: 100%;
  appearance: none;
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 0.65rem 0.85rem;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  color: var(--text-main);
  text-align: left;
  cursor: pointer;
  transition: border-color 140ms ease, background 140ms ease;
}

.dep-link:hover {
  border-color: var(--primary);
  background: var(--bg-subtle);
}

.dep-link:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}

.dep-link__main {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.dep-link__meta {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-shrink: 0;
}

/* --- Mini aperçu de flux sur les cartes du Catalogue --- */
.flow-mini {
  width: 100%;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.3rem;
  padding: 0.5rem 0.6rem;
  border-radius: var(--radius-md);
  background: var(--bg-elevated);
  border: 1px solid var(--border-subtle);
}

.flow-mini__node {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  max-width: 10rem;
  padding: 0.2rem 0.45rem;
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--component-accent, var(--primary));
  font-size: var(--font-size-xs);
  font-weight: 600;
  color: var(--text-main);
}

.flow-mini__label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flow-mini__arrow {
  font-size: var(--font-size-xs);
  color: var(--text-dim);
}

.flow-mini__more {
  font-size: var(--font-size-xs);
  font-weight: 600;
  color: var(--text-dim);
  padding: 0.15rem 0.35rem;
}

/* --- Diagramme visuel d'un flux dans le tiroir d'inspection --- */
.flow-diagram {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  margin: 0.75rem 0 1rem;
}

.flow-diagram__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.flow-diagram__canvas {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 0.35rem;
  padding: 0.9rem;
  border-radius: var(--radius-md);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
}

.flow-diagram__connector {
  display: flex;
  justify-content: center;
  line-height: 1;
}

.flow-diagram__arrow {
  font-size: var(--font-size-sm);
  color: var(--text-dim);
}

.flow-diagram__layer {
  display: grid;
  grid-template-columns: 1fr;
  gap: 0.6rem;
}

.flow-diagram__layer.is-branch {
  grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
}

.flow-diagram__node {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  padding: 0.65rem 0.75rem;
  border-radius: var(--radius-md);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--component-accent, var(--primary));
}

.flow-diagram__ports {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

.flow-diagram__port {
  font-size: var(--font-size-xs);
  font-weight: 700;
  color: var(--primary);
  background: var(--primary-bg);
  padding: 0.1rem 0.45rem;
  border-radius: var(--radius-pill);
}

.flow-diagram__node-head {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
}

.flow-diagram__node-icon {
  width: 1.75rem;
  height: 1.75rem;
  border-radius: var(--radius-sm);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-subtle);
  color: var(--component-accent, var(--primary));
  flex-shrink: 0;
}

.flow-diagram__node-titles {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
}

.flow-diagram__node-title {
  font-size: var(--font-size-sm);
  font-weight: 700;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flow-diagram__node-kind {
  font-size: var(--font-size-xs);
  color: var(--text-dim);
  font-family: var(--font-mono);
}

.flow-diagram__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

@media (max-width: 640px) {
  .store-grid,
  .featured-packs,
  .repo-list {
    grid-template-columns: 1fr;
  }

  .status-banner {
    flex-direction: column;
    align-items: flex-start;
  }
}

/* --- Coloration syntaxique des aperçus de code --- */
.tok-comment { color: var(--text-dim); font-style: italic; }
.tok-string { color: var(--success); }
.tok-number { color: var(--warning); }
.tok-keyword { color: var(--primary); font-weight: 600; }
.tok-function { color: var(--info); }
.tok-variable { color: var(--accent); }
.tok-operator { color: var(--text-dim); }
"#;
