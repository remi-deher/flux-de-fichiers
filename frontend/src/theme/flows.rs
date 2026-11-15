//! Styles de l'atelier de flux (liste des flux, barre de commande, réglages et validation).

pub const FLOWS_CSS: &str = r#"
/* --- Concepteur et Gestionnaire de Flux (Flows) --- */
.form-input--code {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
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
  padding: 0.35rem 0.6rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--text-main);
}

.pipeline-step-node--more {
  background: var(--primary-bg);
  border-color: var(--border-highlight);
  color: var(--primary);
}

.pipeline-connector {
  color: var(--text-muted);
  font-size: 0.85rem;
  user-select: none;
}

/* --- Atelier plein écran (Workbench) --- */
.flow-workbench {
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
  font-size: 0.8rem;
  border-bottom: 1px solid var(--border-subtle);
  background: var(--bg-card);
  color: var(--text-dim);
}

.flow-workbench__readonly {
  background: var(--warning-bg);
  color: var(--warning);
}

/* --- Barre de commande de l'atelier --- */
.flow-command-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem 0.85rem;
  flex-wrap: wrap;
  min-height: 46px;
  padding: 0.35rem 0.75rem;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-color);
  box-shadow: var(--shadow-sm);
  z-index: 20;
  flex-shrink: 0;
}

.flow-command-bar__lead {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
  flex: 1 1 18rem;
}

.flow-command-bar__identity {
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 0.55rem;
  min-width: 0;
  flex-wrap: wrap;
}

.flow-command-bar__title-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
}

.flow-title-btn {
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-main);
  font-size: 1rem;
  font-weight: 700;
  padding: 0.12rem 0.45rem;
  margin-left: -0.45rem;
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  max-width: 340px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: background-color 0.15s ease, border-color 0.15s ease;
}

.flow-title-btn:hover:not(:disabled) {
  background: var(--bg-subtle);
  border-color: var(--border-color);
}

.flow-title-btn:disabled {
  cursor: default;
}

.flow-title-input {
  font-size: 1rem;
  font-weight: 700;
  padding: 0.18rem 0.5rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--primary);
  background: var(--bg-elevated);
  color: var(--text-main);
  min-width: 220px;
  max-width: 340px;
}

.flow-command-bar__pills {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.flow-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.12rem 0.5rem;
  border-radius: var(--radius-pill);
  font-size: 0.72rem;
  font-weight: 600;
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-dim);
  line-height: 1.35;
}

.flow-pill--mask {
  cursor: pointer;
  color: var(--primary);
  background: var(--primary-bg);
  border-color: var(--border-highlight);
}

.flow-pill--mask:hover {
  filter: brightness(1.08);
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
  font-size: 0.7rem;
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
  font-size: 0.85rem;
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
  z-index: 18;
}

.flow-workspace-body.is-panel-open .flow-diagnostics-tray {
  right: 342px;
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
  font-size: 0.88rem;
  font-weight: 700;
  color: var(--text-main);
}

.flow-diagnostics__count {
  font-size: 0.75rem;
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
  font-size: 0.8125rem;
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
  font-size: 0.7rem;
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

/* Sélecteur historique conservé pour compatibilité des tests de thème. */
.studio-toolbar {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
}

@media (max-width: 960px) {
  .flow-settings-drawer {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: min(340px, 100%);
    box-shadow: var(--shadow-lg);
  }
  .flow-diagnostics-tray,
  .flow-workspace-body.is-panel-open .flow-diagnostics-tray {
    left: 10px;
    right: 10px;
    bottom: 10px;
  }
}
"#;
