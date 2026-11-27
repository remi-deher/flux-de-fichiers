//! Styles de l'atelier de flux (liste des flux, barre de commande, réglages et validation).

pub const FLOWS_CSS: &str = r#"
/* --- Concepteur et Gestionnaire de Flux (Flows) --- */
.form-input--code {
  font-family: var(--font-mono);
  font-size: var(--font-size-xs);
  line-height: 1.5;
  resize: vertical;
}

.flow-card__pipeline {
  padding: 0.65rem 0.75rem;
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
}

.flow-pipeline {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.45rem;
}

.pipeline-step-node {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.3rem 0.55rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--step-accent, var(--primary));
  border-radius: var(--radius-sm);
  font-size: var(--font-size-xs);
  font-weight: 600;
  color: var(--text-main);
  max-width: 190px;
}

.pipeline-step-node__icon {
  display: inline-grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--step-accent, var(--primary)) 14%, transparent);
  color: var(--step-accent, var(--primary));
  flex-shrink: 0;
}

.pipeline-step-node__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pipeline-step-node__branch {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0.05rem 0.3rem;
  border-radius: var(--radius-pill);
  background: color-mix(in srgb, var(--step-accent, var(--primary)) 16%, transparent);
  color: var(--step-accent, var(--primary));
  font-size: var(--font-size-xs);
  font-weight: 700;
  line-height: 1;
  flex-shrink: 0;
}

.pipeline-step-node--more {
  border-left-width: 1px;
  background: var(--primary-bg);
  border-color: var(--border-highlight);
  color: var(--primary);
}

.pipeline-connector {
  color: var(--text-muted);
  font-size: var(--font-size-xs);
  user-select: none;
}

/* --- Atelier plein écran (Workbench) --- */
.flow-workbench {
  position: relative;
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  overflow: hidden;
  background: var(--bg-app);
}

.flow-workbench__readonly,
.flow-workbench__feedback {
  margin: 0;
  padding: 0.45rem 1rem;
  font-size: var(--font-size-xs);
  border-bottom: 1px solid var(--border-subtle);
  background: var(--bg-card);
  color: var(--text-dim);
  z-index: 22;
}

.flow-workbench__readonly {
  background: var(--warning-bg);
  color: var(--warning);
}

/* --- Barre de commande flottante en îlots --- */
.flow-command-bar {
  position: absolute;
  top: 12px;
  left: 12px;
  right: 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.65rem;
  flex-wrap: wrap;
  pointer-events: none;
  z-index: 20;
}

.flow-island {
  pointer-events: auto;
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  min-height: 44px;
  padding: 0.3rem 0.6rem;
  background: var(--bg-glass);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  max-width: 100%;
}

.flow-island--identity {
  gap: 0.55rem;
  min-width: 0;
}

.flow-island--tools {
  flex-wrap: wrap;
}

.flow-island--publish {
  margin-left: auto;
}

.flow-island__sep {
  width: 1px;
  height: 20px;
  background: var(--border-subtle);
  margin: 0 0.1rem;
}

.flow-command-bar__identity {
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  flex-wrap: wrap;
}

.flow-command-bar__title-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.flow-title-input {
  font-size: var(--font-size-sm);
  font-weight: 700;
  padding: 0.18rem 0.5rem;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-main);
  min-width: 200px;
  max-width: 280px;
  text-overflow: ellipsis;
  transition: background-color 0.15s ease, border-color 0.15s ease;
}

.flow-title-input:hover:not(:disabled) {
  background: var(--bg-subtle);
  border-color: var(--border-color);
}

.flow-title-input:focus {
  background: var(--bg-elevated);
  border-color: var(--primary);
  outline: none;
}

.flow-command-bar__types {
  min-width: 14rem;
  max-width: 28rem;
}

.flow-command-bar__pills {
  display: flex;
  align-items: center;
  gap: 0.38rem;
  flex-wrap: wrap;
}

.flow-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.12rem 0.5rem;
  border-radius: var(--radius-pill);
  font-size: var(--font-size-xs);
  font-weight: 600;
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-dim);
  line-height: 1.35;
}

.flow-pill--default {
  color: var(--success);
  background: var(--success-bg);
  border-color: color-mix(in srgb, var(--success) 35%, transparent);
}

.flow-pill--state.is-saved {
  color: var(--success);
  background: var(--success-bg);
  border-color: color-mix(in srgb, var(--success) 35%, transparent);
}

.flow-pill--state.is-unsaved {
  color: var(--warning);
  background: var(--warning-bg);
  border-color: color-mix(in srgb, var(--warning) 40%, transparent);
}

.flow-pill--state .status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}

.flow-command-bar__actions {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex-wrap: wrap;
}

.flow-cmd-btn[aria-pressed="true"] {
  background: var(--primary-bg);
  color: var(--primary);
  border-color: var(--border-highlight);
}

.flow-cmd-btn__count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1.15rem;
  height: 1.15rem;
  padding: 0 0.32rem;
  border-radius: var(--radius-pill);
  background: var(--warning-bg);
  color: var(--warning);
  font-size: var(--font-size-xs);
  font-weight: 700;
}

.flow-cmd-btn__count.is-error {
  background: var(--danger-bg);
  color: var(--danger-text);
}

/* --- Zone centrale de travail (Palette + Canevas + Inspecteur/Réglages) --- */
.flow-workspace-body {
  flex: 1;
  display: flex;
  min-height: 0;
  position: relative;
  overflow: hidden;
}

/* --- Tiroir latéral Réglages & Versions --- */
.flow-settings-drawer {
  width: 340px;
  flex-shrink: 0;
  background: var(--bg-card);
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  min-height: 0;
  transition: width 0.2s var(--ease-out);
  z-index: 15;
}

.flow-settings-drawer.is-closed {
  width: 0;
  border-left: none;
  overflow: hidden;
}

.flow-settings__toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  font-size: var(--font-size-xs);
  color: var(--text-main);
  cursor: pointer;
}

/* --- Bandeau inférieur Diagnostics & Validation (flottant sans compresser le canevas) --- */
.flow-diagnostics-tray {
  position: absolute;
  left: 64px;
  right: 196px;
  bottom: 14px;
  background: var(--bg-glass);
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  max-height: 200px;
  overflow-y: auto;
  transition: left 0.2s var(--ease-out), right 0.2s var(--ease-out);
  z-index: 18;
}

.flow-workspace-body.is-palette-open .flow-diagnostics-tray {
  left: 332px;
}

.flow-workspace-body.is-panel-open .flow-diagnostics-tray {
  right: 368px;
}

.flow-diagnostics-tray.is-closed {
  display: none;
}

.flow-diagnostics__inner {
  padding: 0.75rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.flow-diagnostics__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.flow-diagnostics__title-wrap {
  display: flex;
  align-items: center;
  gap: 0.6rem;
}

.flow-diagnostics__title {
  margin: 0;
  font-size: var(--font-size-sm);
  font-weight: 700;
  color: var(--text-main);
}

.flow-diagnostics__count {
  font-size: var(--font-size-xs);
  color: var(--text-dim);
}

.flow-diagnostics__list {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.flow-diagnostic-item {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding: 0.45rem 0.65rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  font-size: var(--font-size-xs);
  color: var(--text-main);
}

.flow-diagnostic-item[data-level="error"] {
  border-color: color-mix(in srgb, var(--danger) 40%, transparent);
  background: var(--danger-bg);
}

.flow-diagnostic-item[data-level="warning"] {
  border-color: color-mix(in srgb, var(--warning) 40%, transparent);
  background: var(--warning-bg);
}

.flow-diagnostic-item[data-level="ok"] {
  border-color: color-mix(in srgb, var(--success) 40%, transparent);
  background: var(--success-bg);
  color: var(--success);
}

.flow-diagnostic-item__level {
  font-size: var(--font-size-xs);
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  flex-shrink: 0;
}

.flow-diagnostic-item__msg {
  flex: 1;
  min-width: 0;
}

.flow-headers__row {
  display: grid;
  grid-template-columns: 1fr 1fr auto;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.flow-condition__group {
  border-left: 2px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding-left: var(--space-1);
  margin-bottom: var(--space-1);
}

.flow-condition__rule {
  grid-template-columns: 1fr 1fr 1fr auto;
}

/* Sélecteur historique conservé pour compatibilité des tests de thème. */
.studio-toolbar {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
}

@media (max-width: 960px) {
  .flow-command-bar {
    top: 8px;
    left: 8px;
    right: 8px;
    gap: 0.4rem;
  }
  .flow-island {
    min-height: 40px;
    padding: 0.25rem 0.5rem;
  }
  .flow-settings-drawer {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: min(340px, 100%);
    box-shadow: var(--shadow-lg);
  }
  .flow-diagnostics-tray,
  .flow-workspace-body.is-palette-open .flow-diagnostics-tray,
  .flow-workspace-body.is-panel-open .flow-diagnostics-tray {
    left: 10px;
    right: 10px;
    bottom: 10px;
  }
}

/* --- Écran de départ d'un nouveau flux --- */
.flow-start {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1rem;
  max-width: 1240px;
  margin: 0 auto;
}

.flow-start__header {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.4rem;
}

.flow-start__title {
  margin: 0;
  font-size: var(--font-size-2xl);
  line-height: 1.1;
}

.flow-start__body {
  display: grid;
  grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr);
  gap: 1rem;
  align-items: start;
}

.flow-start__choices,
.flow-start__side {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  padding: 1rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
}

.flow-start__group-title {
  margin: 0 0 0.5rem;
  font-size: var(--font-size-md);
  color: var(--text-dim);
}

.flow-start__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
  gap: 0.6rem;
}

.flow-start__card {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  min-height: 2.75rem;
  padding: 0.6rem 0.75rem;
  text-align: left;
  color: var(--text-main);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  cursor: pointer;
}

.flow-start__card[aria-pressed="true"] {
  background: var(--primary-bg);
  border-color: var(--primary);
}

.flow-start__card-title {
  font-weight: 600;
}

.flow-start__card-desc {
  font-size: var(--font-size-xs);
  color: var(--text-dim);
}

@media (max-width: 768px) {
  .flow-start__body {
    grid-template-columns: minmax(0, 1fr);
  }
}
"#;
