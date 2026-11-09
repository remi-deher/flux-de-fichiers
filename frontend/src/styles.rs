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

/* Inspecteur de contrats */
.contract-security-banner {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
  gap: 0.75rem;
  margin-bottom: 1.25rem;
}

.security-badge {
  display: flex;
  gap: 0.625rem;
  align-items: center;
  padding: 0.75rem 1rem;
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
}

.security-icon {
  font-size: 1.25rem;
}

.security-badge strong {
  display: block;
  font-size: 0.8125rem;
  color: var(--text-main);
}

.security-badge span {
  display: block;
  font-size: 0.75rem;
  color: var(--text-dim);
}

.contract-selector-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

.contract-tab-btn {
  min-height: 44px;
  padding: 0.5rem 1rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-main);
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.contract-tab-btn:hover {
  border-color: var(--primary);
  background: var(--bg-subtle);
}

.contract-tab-btn.active {
  border-color: var(--primary);
  background: var(--primary);
  color: var(--primary-contrast);
}

.contract-tab-btn:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
}

.contract-display-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.contract-visual-summary {
  padding: 1.25rem;
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-subtle);
}

.visual-summary-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 1rem;
}

.visual-summary-box {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding: 0.875rem;
}

.visual-summary-title {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-main);
  margin-bottom: 0.375rem;
}

.visual-summary-text {
  font-size: 0.75rem;
  color: var(--text-dim);
  line-height: 1.4;
  margin: 0;
}

.macro-pipeline {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
  margin-top: 0.75rem;
}

.pipeline-step {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  padding: 0.625rem 0.875rem;
  display: flex;
  flex-direction: column;
}

.pipeline-step-title {
  font-weight: 600;
  font-size: 0.8125rem;
  color: var(--text-main);
}

.pipeline-step-detail {
  font-size: 0.6875rem;
  color: var(--text-dim);
  font-family: var(--font-mono);
}

.pipeline-arrow {
  font-size: 1.25rem;
  color: var(--primary);
}

.contract-code-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.75rem 1.25rem;
  background: var(--bg-subtle);
  border-bottom: 1px solid var(--border-color);
}

.code-filepath {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  color: var(--text-main);
  font-weight: 600;
}

.code-schema-badge {
  font-size: 0.75rem;
  padding: 0.2rem 0.5rem;
  border-radius: var(--radius-sm);
  background: var(--info-bg);
  color: var(--info-text);
  border: 1px solid var(--info-border);
  margin-left: 0.5rem;
}

.contract-code-wrapper {
  padding: 1rem;
  overflow-x: auto;
  max-height: 420px;
}

.contract-code-wrapper pre {
  margin: 0;
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  line-height: 1.5;
  color: var(--text-main);
}

/* Banc d'essai */
.testbench-layout {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
  gap: 1.5rem;
}

.testbench-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  display: flex;
  flex-direction: column;
}

.testbench-controls {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem;
  margin-bottom: 1.25rem;
  align-items: flex-end;
}

.card-subtitle {
  font-size: 0.8125rem;
  color: var(--text-dim);
  margin-top: 0.25rem;
}

.lifecycle-stepper {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
  gap: 0.5rem;
  margin-bottom: 1.25rem;
}

.step-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.625rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  transition: border-color 0.2s ease, background 0.2s ease;
}

.step-item.active {
  border-color: var(--primary);
  background: var(--info-bg);
}

.step-item.success {
  border-color: var(--success-border);
  background: var(--success-bg);
}

.step-number {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.75rem;
  font-weight: 700;
  background: var(--border-color);
  color: var(--bg-card);
  flex-shrink: 0;
}

.step-item.active .step-number {
  background: var(--primary);
  color: var(--primary-contrast);
}

.step-item.success .step-number {
  background: var(--success-border);
  color: var(--primary-contrast);
}

.step-content {
  display: flex;
  flex-direction: column;
}

.step-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-main);
  line-height: 1.2;
}

.step-status {
  font-size: 0.6875rem;
  color: var(--text-dim);
}

.testbench-console-wrapper {
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  overflow: hidden;
  margin-top: auto;
}

.console-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.375rem 0.75rem;
  border-bottom: 1px solid var(--border-color);
  font-size: 0.75rem;
  color: var(--text-dim);
}

.btn-text-sm {
  background: none;
  border: none;
  color: var(--primary);
  font-size: 0.75rem;
  cursor: pointer;
  padding: 0.125rem 0.375rem;
}

.btn-text-sm:hover {
  text-decoration: underline;
}

.console-log {
  margin: 0;
  padding: 0.75rem;
  font-family: var(--font-mono);
  font-size: 0.75rem;
  line-height: 1.5;
  max-height: 160px;
  overflow-y: auto;
  color: var(--text-main);
  background: transparent;
  white-space: pre-wrap;
}

/* Télémétrie */
.telemetry-metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 1rem;
  margin-top: 0.75rem;
}

.telemetry-metric-tile {
  padding: 1rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.tile-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.tile-label {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-muted);
}

.tile-badge {
  font-size: 0.75rem;
  padding: 0.125rem 0.375rem;
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  color: var(--text-main);
}

.tile-gauge-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.tile-value {
  font-size: 1.35rem;
  font-weight: 700;
  color: var(--text-main);
}

.gauge-bar-track {
  flex: 1;
  height: 10px;
  background: var(--border-color);
  border-radius: 5px;
  overflow: hidden;
}

.gauge-bar-fill {
  height: 100%;
  background: var(--primary);
  border-radius: 5px;
  transition: width 0.3s ease;
}

.tile-subtext {
  font-size: 0.75rem;
  color: var(--text-dim);
}

.storage-mounts-list {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.mount-row {
  display: flex;
  justify-content: space-between;
  font-size: 0.75rem;
}

.mount-name {
  font-weight: 500;
  color: var(--text-main);
}

.mount-space {
  font-family: var(--font-mono);
  color: var(--text-dim);
}

"#;
