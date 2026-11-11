//! Feuilles de styles et variables de design pour le poste de pilotage Flux de Fichiers.

pub const CSS_STYLES: &str = r#"
:root {
  --font-sans: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;

  /* Palette claire : contrastes renforcés (>= 4.5:1 sur texte, >= 3:1 sur composants) */
  --bg-app: #f8fafc;
  --bg-card: #ffffff;
  --bg-subtle: #f1f5f9;
  --border-color: #94a3b8;
  --border-focus: #0369a1;
  --text-main: #0f172a;
  --text-muted: #334155;
  --text-dim: #475569;

  --primary: #0369a1;
  --primary-hover: #075985;
  --primary-contrast: #ffffff;

  --success-bg: #ecfdf5;
  --success-border: #047857;
  --success-text: #064e3b;

  --warning-bg: #fffbeb;
  --warning-border: #b45309;
  --warning-text: #78350f;

  --info-bg: #f0f9ff;
  --info-border: #0284c7;
  --info-text: #075985;

  --danger-bg: #fef2f2;
  --danger-border: #b91c1c;
  --danger-text: #7f1d1d;

  --focus-ring: #0369a1;
  --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
  --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1);
  --radius-sm: 6px;
  --radius-md: 10px;
  --radius-lg: 14px;
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg-app: #0b0f17;
    --bg-card: #151d2c;
    --bg-subtle: #1e293b;
    --border-color: #64748b;
    --border-focus: #38bdf8;
    --text-main: #f8fafc;
    --text-muted: #e2e8f0;
    --text-dim: #cbd5e1;

    --primary: #38bdf8;
    --primary-hover: #7dd3fc;
    --primary-contrast: #0f172a;

    --success-bg: #064e3b;
    --success-border: #34d399;
    --success-text: #ecfdf5;

    --warning-bg: #78350f;
    --warning-border: #fbbf24;
    --warning-text: #fffbeb;

    --info-bg: #0c4a6e;
    --info-border: #38bdf8;
    --info-text: #f0f9ff;

    --danger-bg: #7f1d1d;
    --danger-border: #f87171;
    --danger-text: #fef2f2;

    --focus-ring: #38bdf8;
    --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.3);
    --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.4), 0 2px 4px -2px rgb(0 0 0 / 0.3);
  }
}

:root[data-theme="dark"] {
  --bg-app: #0b0f17;
  --bg-card: #151d2c;
  --bg-subtle: #1e293b;
  --border-color: #64748b;
  --border-focus: #38bdf8;
  --text-main: #f8fafc;
  --text-muted: #e2e8f0;
  --text-dim: #cbd5e1;

  --primary: #38bdf8;
  --primary-hover: #7dd3fc;
  --primary-contrast: #0f172a;

  --success-bg: #064e3b;
  --success-border: #34d399;
  --success-text: #ecfdf5;

  --warning-bg: #78350f;
  --warning-border: #fbbf24;
  --warning-text: #fffbeb;

  --info-bg: #0c4a6e;
  --info-border: #38bdf8;
  --info-text: #f0f9ff;

  --danger-bg: #7f1d1d;
  --danger-border: #f87171;
  --danger-text: #fef2f2;

  --focus-ring: #38bdf8;
  --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.3);
  --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.4), 0 2px 4px -2px rgb(0 0 0 / 0.3);
}

:root[data-theme="light"] {
  --bg-app: #f8fafc;
  --bg-card: #ffffff;
  --bg-subtle: #f1f5f9;
  --border-color: #94a3b8;
  --border-focus: #0369a1;
  --text-main: #0f172a;
  --text-muted: #334155;
  --text-dim: #475569;

  --primary: #0369a1;
  --primary-hover: #075985;
  --primary-contrast: #ffffff;

  --success-bg: #ecfdf5;
  --success-border: #047857;
  --success-text: #064e3b;

  --warning-bg: #fffbeb;
  --warning-border: #b45309;
  --warning-text: #78350f;

  --info-bg: #f0f9ff;
  --info-border: #0284c7;
  --info-text: #075985;

  --danger-bg: #fef2f2;
  --danger-border: #b91c1c;
  --danger-text: #7f1d1d;

  --focus-ring: #0369a1;
  --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
  --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1);
}

/* Reset et typographie de base */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

body {
  font-family: var(--font-sans);
  background-color: var(--bg-app);
  color: var(--text-main);
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  min-height: 100vh;
}

/* Lien d'évitement accessible */
.skip-link {
  position: absolute;
  top: -999px;
  left: 1rem;
  background: var(--primary);
  color: var(--primary-contrast);
  padding: 0.75rem 1.25rem;
  font-weight: 700;
  text-decoration: none;
  border-radius: var(--radius-sm);
  z-index: 9999;
}

.skip-link:focus {
  top: 1rem;
}

.app-container {
  max-width: 1200px;
  margin: 0 auto;
  padding: 1.25rem 1rem 3rem 1rem;
}

/* Header */
.app-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 1rem;
  padding-bottom: 1rem;
  border-bottom: 1px solid var(--border-color);
  margin-bottom: 1.25rem;
}

.brand-wrapper {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.brand-badge {
  display: inline-block;
  background: var(--primary);
  color: var(--primary-contrast);
  font-weight: 700;
  font-size: 0.75rem;
  padding: 0.25rem 0.6rem;
  border-radius: var(--radius-sm);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

h1.app-title {
  font-size: 1.5rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--text-main);
}

.header-status {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

/* Barre de navigation par onglets (Tabs) */
.tab-navigation {
  margin-bottom: 1.5rem;
  border-bottom: 1px solid var(--border-color);
}

.tab-list {
  display: flex;
  gap: 0.5rem;
  overflow-x: auto;
  padding-bottom: 2px;
  scrollbar-width: thin;
}

.tab-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem 1rem;
  font-family: var(--font-sans);
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--text-muted);
  background: transparent;
  border: none;
  border-bottom: 3px solid transparent;
  cursor: pointer;
  min-height: 44px;
  min-width: 44px;
  white-space: nowrap;
  transition: color 0.15s ease, border-color 0.15s ease;
}

.tab-btn:hover {
  color: var(--text-main);
}

.tab-btn[aria-selected="true"] {
  color: var(--primary);
  border-bottom-color: var(--primary);
}

.tab-btn:focus-visible {
  outline: 3px solid var(--focus-ring);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}

.tab-panel {
  display: block;
}

.tab-panel[hidden] {
  display: none;
}

/* Badges d'état */
.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.8125rem;
  font-weight: 600;
  padding: 0.35rem 0.75rem;
  border-radius: 9999px;
  border: 1px solid transparent;
}

.status-pill.online,
.status-pill.succeeded {
  background: var(--success-bg);
  border-color: var(--success-border);
  color: var(--success-text);
}

.status-pill.offline,
.status-pill.failed {
  background: var(--danger-bg);
  border-color: var(--danger-border);
  color: var(--danger-text);
}

.status-pill.warning,
.status-pill.queued {
  background: var(--warning-bg);
  border-color: var(--warning-border);
  color: var(--warning-text);
}

.status-pill.info,
.status-pill.running {
  background: var(--info-bg);
  border-color: var(--info-border);
  color: var(--info-text);
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: currentColor;
}

/* Grilles et panneaux */
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1rem;
  margin-bottom: 1.5rem;
}

.metric-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  box-shadow: var(--shadow-sm);
}

.metric-label {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-dim);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.35rem;
}

.metric-value {
  font-size: 1.75rem;
  font-weight: 700;
  color: var(--text-main);
  font-family: var(--font-mono);
}

.metric-subtitle {
  font-size: 0.8125rem;
  color: var(--text-dim);
  margin-top: 0.25rem;
}

.section-panel {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.5rem;
  margin-bottom: 1.5rem;
  box-shadow: var(--shadow-sm);
}

.section-panel-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  flex-wrap: wrap;
  gap: 1rem;
  margin-bottom: 1.25rem;
}

.section-title {
  font-size: 1.25rem;
  font-weight: 700;
  color: var(--text-main);
  letter-spacing: -0.01em;
}

.section-subtitle {
  font-size: 0.875rem;
  color: var(--text-dim);
  margin-top: 0.25rem;
}

/* Barres d'outils et recherche */
.toolbar-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
  margin-bottom: 1rem;
}

.search-box {
  position: relative;
  display: inline-flex;
  align-items: center;
  flex: 1;
  min-width: 240px;
  max-width: 400px;
}

.search-input {
  width: 100%;
  font-family: var(--font-sans);
  font-size: 0.875rem;
  padding: 0.625rem 0.875rem 0.625rem 2.25rem;
  min-height: 44px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-main);
}

.search-icon {
  position: absolute;
  left: 0.75rem;
  color: var(--text-dim);
  pointer-events: none;
}

.filter-group {
  display: flex;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.filter-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.35rem 0.75rem;
  min-height: 44px;
  min-width: 44px;
  border-radius: 9999px;
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-main);
  font-size: 0.8125rem;
  font-weight: 600;
  cursor: pointer;
}

.filter-chip[aria-pressed="true"] {
  background: var(--primary);
  color: var(--primary-contrast);
  border-color: var(--primary);
}

.filter-chip:focus-visible {
  outline: 3px solid var(--focus-ring);
  outline-offset: 2px;
}

/* Tableaux de données */
.table-wrapper {
  overflow-x: auto;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
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
  vertical-align: middle;
}

.data-table th {
  background: var(--bg-subtle);
  font-weight: 600;
  color: var(--text-dim);
  font-size: 0.8125rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.data-table tr:last-child td {
  border-bottom: none;
}

.data-table tr:hover td {
  background-color: var(--bg-subtle);
}

.empty-state-text {
  text-align: center;
  padding: 2.5rem 1rem !important;
  color: var(--text-dim);
  font-style: italic;
}

/* Boutons et formulaires */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  font-family: var(--font-sans);
  font-size: 0.875rem;
  font-weight: 600;
  padding: 0.625rem 1rem;
  min-height: 44px;
  min-width: 44px;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  cursor: pointer;
  text-decoration: none;
  transition: background-color 0.15s ease, border-color 0.15s ease;
}

.btn:focus-visible {
  outline: 3px solid var(--focus-ring);
  outline-offset: 2px;
}

.btn-primary {
  background: var(--primary);
  color: var(--primary-contrast);
}

.btn-primary:hover:not(:disabled) {
  background: var(--primary-hover);
}

.btn-secondary {
  background: var(--bg-subtle);
  border-color: var(--border-color);
  color: var(--text-main);
}

.btn-secondary:hover:not(:disabled) {
  background: var(--border-color);
}

.btn-sm {
  font-size: 0.8125rem;
  padding: 0.5rem 0.75rem;
  min-height: 44px;
  min-width: 44px;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  margin-bottom: 1rem;
}

.form-label {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text-main);
}

.form-input,
.form-select {
  font-family: var(--font-mono);
  font-size: 0.875rem;
  padding: 0.625rem 0.875rem;
  min-height: 44px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-main);
}

.form-input:focus,
.form-select:focus,
.search-input:focus {
  outline: 3px solid var(--focus-ring);
  outline-offset: 2px;
  border-color: var(--border-focus);
}

.form-help {
  font-size: 0.8125rem;
  color: var(--text-dim);
}

/* Modale dialog HTML5 */
dialog.app-dialog {
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  background: var(--bg-card);
  color: var(--text-main);
  padding: 0;
  max-width: 600px;
  width: calc(100% - 2rem);
  box-shadow: var(--shadow-md);
  margin: auto;
}

dialog.app-dialog::backdrop {
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(2px);
}

.dialog-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1.25rem 1.5rem;
  border-bottom: 1px solid var(--border-color);
}

.dialog-title {
  font-size: 1.125rem;
  font-weight: 700;
}

.dialog-body {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  max-height: 70vh;
  overflow-y: auto;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
  padding: 1rem 1.5rem;
  border-top: 1px solid var(--border-color);
  background: var(--bg-subtle);
  border-bottom-left-radius: var(--radius-md);
  border-bottom-right-radius: var(--radius-md);
}

/* Grille de détail de tâche */
.detail-grid {
  display: grid;
  grid-template-columns: 140px 1fr;
  gap: 0.75rem 1rem;
  font-size: 0.875rem;
}

.detail-term {
  font-weight: 600;
  color: var(--text-dim);
}

.detail-val {
  font-family: var(--font-mono);
  word-break: break-all;
  color: var(--text-main);
}

.code-snippet {
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding: 0.75rem;
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  overflow-x: auto;
  user-select: all;
  white-space: pre-wrap;
  word-break: break-all;
}

/* Toast Notice */
.toast-notice {
  display: none;
  padding: 0.75rem 1.25rem;
  border-radius: var(--radius-sm);
  font-size: 0.875rem;
  font-weight: 600;
  margin-bottom: 1rem;
}

.toast-notice.visible {
  display: block;
}

.toast-notice.info {
  background: var(--info-bg);
  border: 1px solid var(--info-border);
  color: var(--info-text);
}

.toast-notice.error {
  background: var(--danger-bg);
  border: 1px solid var(--danger-border);
  color: var(--danger-text);
}

.toast-notice.success {
  background: var(--success-bg);
  border: 1px solid var(--success-border);
  color: var(--success-text);
}

/* Cartes fonctionnalités futures */
.future-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 1rem;
}

.future-card {
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  background: var(--bg-subtle);
  opacity: 0.85;
}

.future-badge {
  display: inline-block;
  font-size: 0.75rem;
  font-weight: 700;
  padding: 0.2rem 0.5rem;
  border-radius: var(--radius-sm);
  background: var(--warning-bg);
  color: var(--warning-text);
  border: 1px solid var(--warning-border);
  margin-bottom: 0.5rem;
  text-transform: uppercase;
}

/* Adaptation petits écrans */
@media (max-width: 640px) {
  .app-header {
    flex-direction: column;
    align-items: flex-start;
  }
  .header-status {
    width: 100%;
    justify-content: space-between;
  }
  .detail-grid {
    grid-template-columns: 1fr;
    gap: 0.25rem;
  }
  .toolbar-bar {
    flex-direction: column;
    align-items: stretch;
  }
  .search-box {
    max-width: 100%;
  }
}

/* --- Section Plugins, Outils et Contrats --- */
.tools-filter-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
}

.tools-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
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
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.tool-card:hover {
  border-color: var(--border-focus);
  box-shadow: var(--shadow-sm);
}

.tool-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 0.75rem;
}

.tool-identity {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}

.tool-badge-icon {
  font-size: 1.5rem;
  line-height: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
}

.tool-card-title {
  font-size: 1.05rem;
  font-weight: 600;
  color: var(--text-main);
  margin: 0;
}

.tool-card-id {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--text-dim);
}

.tool-card-desc {
  font-size: 0.875rem;
  color: var(--text-muted);
  margin: 0.875rem 0;
  line-height: 1.5;
  flex: 1;
}

.tool-meta-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
  margin-bottom: 1rem;
}

.meta-tag {
  font-size: 0.75rem;
  padding: 0.2rem 0.5rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-muted);
  border: 1px solid var(--border-color);
}

.meta-tag.active-choice {
  background: var(--info-bg);
  color: var(--info-text);
  border-color: var(--info-border);
  font-weight: 600;
}

.tool-card-footer {
  display: flex;
  gap: 0.5rem;
  justify-content: flex-end;
  border-top: 1px solid var(--border-color);
  padding-top: 0.875rem;
}

.copy-input-row {
  display: flex;
  gap: 0.75rem;
  flex-wrap: wrap;
  align-items: center;
}

.workers-info-grid {
  margin-top: 1.5rem;
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 1rem;
}

.card-caption {
  font-size: 0.875rem;
  color: var(--text-dim);
  margin-top: 0.25rem;
}

.code-block-wrapper {
  background: var(--surface-hover);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0.875rem 1rem;
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  color: var(--text-main);
  white-space: pre-wrap;
  word-break: break-all;
}

/* --- Concepteur et Gestionnaire de Flux (Flows) --- */
.flows-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 1.25rem;
  margin-top: 1rem;
}

.flow-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.flow-card:hover {
  border-color: var(--primary);
  box-shadow: var(--shadow-sm);
}

.flow-card.active-flow {
  border-left: 4px solid var(--primary);
}

.flow-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.flow-card-title {
  font-size: 1.125rem;
  font-weight: 700;
  color: var(--text-main);
  margin: 0;
}

.flow-card-desc {
  font-size: 0.875rem;
  color: var(--text-dim);
  margin: 0.35rem 0 0 0;
  line-height: 1.45;
}

.flow-pipeline-container {
  overflow-x: auto;
  padding: 0.75rem 0;
}

.flow-pipeline {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  min-width: max-content;
}

.flow-block-step {
  display: flex;
  flex-direction: column;
  padding: 0.5rem 0.75rem;
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  min-width: 170px;
}

.flow-block-step.implicit-step {
  border-left: 3px solid var(--info-border);
  background: var(--info-bg);
}

.step-badge {
  font-size: 0.6875rem;
  font-weight: 700;
  color: var(--primary);
  text-transform: uppercase;
  margin-bottom: 0.2rem;
}

.step-name {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-main);
}

.step-type {
  font-size: 0.6875rem;
  color: var(--text-dim);
  margin-top: 0.15rem;
}

.flow-arrow {
  color: var(--text-dim);
  font-weight: bold;
  font-size: 1rem;
}

.flow-card-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.75rem;
  border-top: 1px solid var(--border-color);
  padding-top: 0.875rem;
  flex-wrap: wrap;
}

.flow-pattern-tag {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--text-dim);
  background: var(--bg-subtle);
  padding: 0.25rem 0.5rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
}

.flow-editor-steps {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-top: 0.5rem;
}

.editor-step-card {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.625rem 0.875rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
}

.editor-step-card.implicit-step {
  background: var(--info-bg);
  border-color: var(--info-border);
}

.step-order {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--primary);
  color: var(--primary-contrast);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.75rem;
  font-weight: bold;
  flex-shrink: 0;
}

.step-subtext {
  font-size: 0.75rem;
  color: var(--text-dim);
  margin-top: 0.15rem;
}

/* --- Canvas Visuel No-Code (Zapier / FileFlows style) --- */
.canvas-header-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.875rem 1.25rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  border-bottom: none;
}

.canvas-workspace-layout {
  display: flex;
  height: 620px;
  border: 1px solid var(--border-color);
  border-radius: 0 0 var(--radius-md) var(--radius-md);
  overflow: hidden;
  background: var(--bg-card);
}

.canvas-blocks-sidebar {
  width: 260px;
  background: var(--bg-subtle);
  border-right: 1px solid var(--border-color);
  padding: 1rem;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.sidebar-block-group {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.sidebar-group-title {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--text-dim);
  letter-spacing: 0.05em;
}

.draggable-block-item {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  padding: 0.625rem 0.75rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.2s ease;
  user-select: none;
}

.draggable-block-item:hover {
  border-color: var(--primary);
  background: var(--info-bg);
}

.draggable-block-item.addable .block-add-plus {
  margin-left: auto;
  font-weight: bold;
  font-size: 1.1rem;
  color: var(--primary);
}

.block-drag-icon {
  font-size: 1.25rem;
}

.block-drag-info {
  display: flex;
  flex-direction: column;
  font-size: 0.8125rem;
}

.block-drag-info strong {
  color: var(--text-main);
}

.block-drag-info span {
  font-size: 0.6875rem;
  color: var(--text-dim);
}

.sidebar-help-tip {
  margin-top: auto;
  padding: 0.625rem;
  background: var(--bg-card);
  border-radius: var(--radius-sm);
  border: 1px dashed var(--border-color);
  font-size: 0.75rem;
  color: var(--text-dim);
}

.canvas-board-wrapper {
  flex: 1;
  position: relative;
  background-color: var(--bg-app);
  background-image: radial-gradient(circle, var(--border-color) 1px, transparent 1px);
  background-size: 24px 24px;
  overflow-y: auto;
  padding: 2.5rem 1.5rem;
  display: flex;
  justify-content: center;
}

.canvas-connections-svg {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}

.canvas-nodes-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2.25rem;
  width: 100%;
  max-width: 440px;
  position: relative;
  z-index: 1;
}

.canvas-node {
  width: 100%;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  padding: 1rem 1.25rem;
  position: relative;
  cursor: pointer;
  transition: all 0.2s ease;
}

.canvas-node:hover {
  border-color: var(--primary);
  box-shadow: var(--shadow-md);
}

.canvas-node.selected {
  border-color: var(--primary);
  box-shadow: 0 0 0 2px var(--primary);
}

.canvas-node.implicit-node {
  border-left: 4px solid var(--info-border);
}

.node-connector {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--primary);
  border: 2px solid var(--bg-card);
  position: absolute;
  z-index: 2;
}

.connector-in {
  top: -7px;
  left: 50%;
  transform: translateX(-50%);
}

.connector-out {
  bottom: -7px;
  left: 50%;
  transform: translateX(-50%);
}

.node-multi-outputs {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  margin-top: 0.75rem;
  padding-top: 0.5rem;
  border-top: 1px dashed var(--border-color);
  position: relative;
}

.multi-output-port {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  position: relative;
  padding-bottom: 6px;
}

.port-label {
  font-size: 0.6875rem;
  font-weight: 700;
  white-space: nowrap;
}

.port-true .port-label {
  color: #10b981;
}

.port-false .port-label {
  color: #f59e0b;
}

.port-fallback .port-label {
  color: #8b5cf6;
}

.connector-out.out-true {
  background: #10b981;
  position: absolute;
  bottom: -13px;
  left: 50%;
  transform: translateX(-50%);
}

.connector-out.out-false {
  background: #f59e0b;
  position: absolute;
  bottom: -13px;
  left: 50%;
  transform: translateX(-50%);
}

.connector-out.out-fallback {
  background: #8b5cf6;
  position: absolute;
  bottom: -13px;
  left: 50%;
  transform: translateX(-50%);
}

.node-header {
  display: flex;
  align-items: center;
  gap: 0.625rem;
}

.node-icon {
  font-size: 1.35rem;
}

.node-title-box {
  flex: 1;
}

.node-kind {
  font-size: 0.6875rem;
  text-transform: uppercase;
  color: var(--text-dim);
  letter-spacing: 0.04em;
  display: block;
}

.node-name {
  margin: 0;
  font-size: 0.9375rem;
  font-weight: 700;
  color: var(--text-main);
}

.node-remove-btn {
  background: none;
  border: none;
  color: var(--text-dim);
  font-size: 1rem;
  cursor: pointer;
  padding: 0.25rem 0.5rem;
  border-radius: var(--radius-sm);
}

.node-remove-btn:hover {
  color: var(--danger-text);
  background: var(--danger-bg);
}

.node-summary {
  font-size: 0.75rem;
  color: var(--text-dim);
  margin-top: 0.5rem;
  line-height: 1.4;
  border-top: 1px solid var(--border-color);
  padding-top: 0.5rem;
}

.canvas-inspector-sidebar {
  width: 300px;
  background: var(--bg-card);
  border-left: 1px solid var(--border-color);
  padding: 1.25rem;
  overflow-y: auto;
}

.inspector-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.25rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid var(--border-color);
}

.inspector-title {
  margin: 0;
  font-size: 1rem;
  font-weight: 700;
  color: var(--text-main);
}

/* Modale de Template Wizard */
.wizard-templates-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 0.875rem;
  margin-top: 1rem;
}

.wizard-template-card {
  display: flex;
  gap: 0.875rem;
  align-items: flex-start;
  padding: 1rem;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  cursor: pointer;
  transition: all 0.2s ease;
}

.wizard-template-card:hover {
  border-color: var(--primary);
  background: var(--bg-subtle);
}

.wizard-template-card.active {
  border-color: var(--primary);
  background: var(--info-bg);
  box-shadow: 0 0 0 1px var(--primary);
}

.template-icon {
  font-size: 1.75rem;
}

.template-title {
  margin: 0;
  font-size: 0.9375rem;
  font-weight: 700;
  color: var(--text-main);
}

.template-desc {
  margin: 0.3rem 0 0 0;
  font-size: 0.75rem;
  color: var(--text-dim);
  line-height: 1.45;
}


"#;
