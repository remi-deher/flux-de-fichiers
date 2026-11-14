//! Styles des composants partagés de l'interface en Rust typé.

pub const COMPONENTS_CSS: &str = r#"
/* --- Boutons d'action --- */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  min-height: var(--touch-target-compact);
  padding: 0.5rem 1rem;
  font-size: 0.875rem;
  font-weight: 600;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  cursor: pointer;
  transition: all 0.15s ease-in-out;
  text-decoration: none;
  line-height: 1.25;
  white-space: nowrap;
  user-select: none;
  touch-action: manipulation;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-primary {
  background-color: var(--primary);
  color: var(--primary-contrast);
  box-shadow: var(--shadow-sm);
}

.btn-primary:hover:not(:disabled) {
  opacity: 0.9;
  box-shadow: var(--shadow-md);
}

.btn-secondary {
  background-color: var(--bg-subtle);
  color: var(--text-main);
  border-color: var(--border-color);
}

.btn-secondary:hover:not(:disabled) {
  background-color: var(--bg-card);
  border-color: var(--primary);
}

/* Vue d'ensemble des paramètres : tuile cliquable et lignes « à surveiller ». */
.settings-tile {
  display: block;
  width: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.settings-tile:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}

.settings-tile:hover .metric-card {
  border-color: var(--primary);
}

.settings-section[hidden] {
  display: none;
}

.settings-alert {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.6rem 0;
  border-top: 1px solid var(--border-color);
  font-size: 0.875rem;
}

.settings-alert:first-child {
  border-top: 0;
}

.settings-alert__dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--success);
}

.settings-alert[data-tone="warning"] .settings-alert__dot {
  background-color: var(--warning);
}

.settings-alert__text {
  flex: 1;
  min-width: 0;
}

/* Retour visuel d'une copie réussie : le bouton reste en « Copié ✓ » un instant. */
.btn[data-copied="true"] {
  background-color: var(--success-bg);
  color: var(--success);
  border-color: var(--success);
}

/* Zone de texte hors champ utilisée pour copier en HTTP simple (jamais visible). */
.clipboard-proxy {
  position: fixed;
  top: 0;
  left: 0;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.btn-danger {
  background-color: var(--danger-bg);
  color: var(--danger);
  border-color: var(--danger);
}

.btn-danger:hover:not(:disabled) {
  background-color: var(--danger);
  color: #ffffff;
}

.btn-sm {
  padding: 0.3rem 0.65rem;
  font-size: 0.75rem;
}

/* --- Badges et Pilules d'état (Status Pills) --- */
.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.2rem 0.6rem;
  border-radius: var(--radius-pill);
  font-size: 0.75rem;
  font-weight: 600;
  line-height: 1;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: currentColor;
}

.status-pill.online,
.status-pill.success {
  background-color: var(--success-bg);
  color: var(--success);
}

.status-pill.warning {
  background-color: var(--warning-bg);
  color: var(--warning);
}

.status-pill.offline,
.status-pill.error,
.status-pill.danger {
  background-color: var(--danger-bg);
  color: var(--danger);
}

.status-pill.info {
  background-color: var(--info-bg);
  color: var(--info);
}

/* --- Tableaux de données accessibles --- */
.table-wrapper {
  overflow-x: auto;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background-color: var(--bg-card);
  margin-top: 1rem;
}

.data-table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: 0.875rem;
}

.data-table th,
.data-table td {
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--border-color);
}

.data-table th {
  background-color: var(--bg-subtle);
  font-weight: 600;
  color: var(--text-dim);
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.data-table tr:last-child td {
  border-bottom: none;
}

.data-table tr:hover td,
.data-table tr.is-open td {
  background-color: var(--bg-subtle);
}

.registre {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.registre > .table-wrapper {
  margin-top: 0;
}

.inspection-drawer__actions {
  display: flex;
  gap: 0.5rem;
  flex: none;
}

.library-worker-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.library-worker-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
  padding: 0.6rem 0.75rem;
  background: var(--bg-subtle);
  border-radius: var(--radius-sm);
}

.library-worker-path {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  color: var(--text-dim);
}

.form-input--key {
  flex: 1;
  font-family: var(--font-mono);
  font-size: 0.875rem;
}

.inspection-drawer {
  position: fixed;
  inset: 0;
  z-index: 60;
  visibility: hidden;
  pointer-events: none;
}

.inspection-drawer.is-open {
  visibility: visible;
  pointer-events: auto;
}

.inspection-drawer__scrim {
  position: absolute;
  inset: 0;
  border: 0;
  padding: 0;
  background: rgba(0, 0, 0, 0.45);
  cursor: pointer;
}

.inspection-drawer__panel {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  width: min(100%, max(24rem, 66.666%));
  background: var(--bg-card);
  border-left: 1px solid var(--border-color);
  box-shadow: var(--shadow-md);
  transform: translateX(100%);
  outline: none;
}

.inspection-drawer.is-open .inspection-drawer__panel {
  transform: none;
}

.inspection-drawer__header {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 1rem 1rem 0.85rem;
  border-bottom: 1px solid var(--border-color);
}

.inspection-drawer__identity {
  flex: 1;
  min-width: 0;
}

.inspection-drawer__eyebrow {
  margin: 0 0 0.2rem;
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-dim);
}

.inspection-drawer__header .header-icon-btn {
  flex: none;
}

.inspection-drawer__subnav {
  display: flex;
  border-bottom: 1px solid var(--border-color);
}

.inspection-drawer__tab {
  flex: 1;
  appearance: none;
  border: 0;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  padding: 0.8rem 0.7rem;
  text-align: center;
  background: transparent;
  color: var(--text-dim);
  font: inherit;
  font-size: 0.8125rem;
  font-weight: 700;
  cursor: pointer;
}

.inspection-drawer__tab.is-active {
  color: var(--text-main);
  border-bottom-color: var(--primary);
}

.inspection-drawer__tab:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
}

.inspection-drawer__body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 1rem;
}

.inspection-drawer__pane {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.inspection-panel__title {
  margin: 0;
  font-size: 1.125rem;
  line-height: 1.3;
  font-weight: 700;
  color: var(--text-main);
}

.inspection-panel__facts {
  margin: 0;
  font-size: 0.875rem;
  line-height: 1.5;
  color: var(--text-dim);
}

.inspection-panel__action {
  align-self: stretch;
}

.empty-state-text {
  text-align: center;
  padding: 2rem 1rem !important;
  color: var(--text-dim);
}

/* --- Cartes de métriques et panneaux d'information --- */
.metric-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  background-color: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: var(--space-3);
  box-shadow: var(--shadow-sm);
  transition: transform 0.2s ease, box-shadow 0.2s ease, border-color 0.2s ease;
}

.metric-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
  border-color: var(--text-muted);
}

.metric-card[data-tone="success"] { --tone: var(--success); --tone-bg: var(--success-bg); }
.metric-card[data-tone="warning"] { --tone: var(--warning); --tone-bg: var(--warning-bg); }
.metric-card[data-tone="danger"] { --tone: var(--danger); --tone-bg: var(--danger-bg); }

.metric-head {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.metric-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: var(--radius-sm);
  background-color: var(--tone-bg, var(--primary-bg));
  color: var(--tone, var(--primary));
}

.metric-label {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-dim);
}

.metric-value {
  font-size: 2rem;
  font-weight: 700;
  line-height: 1.1;
  color: var(--text-main);
  font-family: var(--font-mono);
}

.metric-card[data-tone="success"] .metric-value,
.metric-card[data-tone="warning"] .metric-value,
.metric-card[data-tone="danger"] .metric-value {
  color: var(--tone);
}

.metric-subtitle {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.card-caption {
  font-size: 0.8125rem;
  color: var(--text-dim);
  margin-top: 0.5rem;
  line-height: 1.45;
}

.workers-info-grid,
.metrics-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 1rem;
  margin-top: 1.25rem;
}

/* --- Panneaux de section (Sections structurées) --- */
.section-panel {
  background-color: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.5rem;
  box-shadow: var(--shadow-sm);
  margin-bottom: 1.5rem;
}

.section-panel-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 1.25rem;
  gap: 1rem;
}

.section-panel-header__actions {
  flex: none;
}

.section-title {
  font-size: 1.25rem;
  font-weight: 700;
  color: var(--text-main);
}

.section-subtitle {
  font-size: 0.875rem;
  color: var(--text-dim);
  margin-top: 0.25rem;
  line-height: 1.45;
}

/* --- Formulaires et Champs de saisie --- */
.form-group {
  margin-bottom: 1.25rem;
}

.form-label {
  display: block;
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-main);
  margin-bottom: 0.35rem;
}

.form-help {
  font-size: 0.75rem;
  color: var(--text-dim);
  margin-top: 0.35rem;
}

.form-input,
.form-select {
  width: 100%;
  padding: 0.6rem 0.75rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background-color: var(--bg-subtle);
  color: var(--text-main);
  font-size: 0.875rem;
  transition: border-color 0.15s ease-in-out;
  box-sizing: border-box;
}

.form-input:focus,
.form-select:focus {
  outline: none;
  border-color: var(--primary);
  background-color: var(--bg-card);
}

.copy-input-row {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

/* --- Blocs de code & Détails techniques --- */
.code-block-wrapper {
  background-color: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding: 0.875rem;
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: 0.8125rem;
}

.code-snippet {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  padding: 0.2rem 0.4rem;
  background-color: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
}

.detail-grid {
  display: grid;
  grid-template-columns: 220px 1fr;
  gap: 0.75rem 1.5rem;
  font-size: 0.875rem;
}

@media (max-width: 640px) {
  .detail-grid {
    grid-template-columns: 1fr;
    gap: 0.25rem;
  }
}

.detail-term {
  color: var(--text-dim);
  font-weight: 500;
}

.detail-val {
  color: var(--text-main);
  font-weight: 600;
}

/* --- Modales natives (<dialog>) --- */
dialog.app-dialog {
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  background-color: var(--bg-card);
  color: var(--text-main);
  padding: 1.5rem;
  max-width: 600px;
  width: 90%;
  margin: auto;
  box-shadow: var(--shadow-md);
}

dialog.app-dialog::backdrop {
  background-color: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(3px);
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.25rem;
}

.dialog-title {
  font-size: 1.25rem;
  font-weight: 700;
}

.dialog-body {
  margin-bottom: 1.5rem;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
}

.dialog-footer-ruled {
  padding-top: 1rem;
  border-top: 1px solid var(--border-color);
  margin-top: 1rem;
}

.text-danger { color: var(--danger-text); }
.text-strong { font-weight: 600; color: var(--text-main); }
.text-dim { color: var(--text-dim); }
.text-primary { color: var(--primary); }
.text-mono { font-family: var(--font-mono); }
.text-mono-strong { font-family: var(--font-mono); font-weight: 600; }
.text-sm { font-size: 0.8125rem; }
.text-xs { font-size: 0.75rem; color: var(--text-dim); }
.cell-end { text-align: right; }
.inline-actions { display: inline-flex; gap: 0.4rem; }
.cluster { display: flex; align-items: center; gap: 0.5rem; }
.cluster-wrap { flex-wrap: wrap; }
.cluster-end { display: flex; justify-content: flex-end; gap: 0.75rem; margin-top: 1rem; }
.cluster-spread { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
.cluster-center { display: flex; gap: 0.75rem; justify-content: center; flex-wrap: wrap; }
.truncate {
  font-size: 0.8125rem;
  font-family: var(--font-mono);
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.empty-state-panel {
  grid-column: 1 / -1;
  text-align: center;
  padding: 2.5rem 1rem;
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-md);
  background: var(--bg-card);
}
.empty-state-icon { font-size: 2rem; margin-bottom: 0.5rem; display: block; }
.empty-state-title { margin: 0 0 0.5rem 0; font-size: 1.125rem; font-weight: 600; color: var(--text-main); }
.empty-state-message { margin: 0 auto 1.25rem auto; max-width: 500px; font-size: 0.875rem; color: var(--text-muted); }
dialog.app-dialog.dialog-narrow { max-width: 540px; }
.form-grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem; }
.btn-danger-quiet { color: var(--danger-text); padding: 0.2rem 0.5rem; font-size: 0.75rem; margin-left: 0.5rem; }
.is-filtered-out { display: none !important; }
.no-margin-top { margin-top: 0; }

/* --- Toasts & Notifications flottantes --- */
.toast-notice {
  padding: 0.75rem 1rem;
  border-radius: var(--radius-sm);
  font-size: 0.875rem;
  margin: 1rem 1.5rem 0 1.5rem;
  display: none;
}

.toast-notice.info {
  background-color: var(--info-bg);
  border: 1px solid var(--info);
  color: var(--info);
}

.toast-notice.success {
  background-color: var(--success-bg);
  border: 1px solid var(--success);
  color: var(--success);
}

.toast-notice.error {
  background-color: var(--danger-bg);
  border: 1px solid var(--danger);
  color: var(--danger);
}
"#;
