//! Feuilles de styles et variables de design pour Flux de Fichiers.

pub const CSS_STYLES: &str = r#"
:root {
  --font-sans: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;

  /* Palette claire : contrastes renforcés (>= 4.5:1 sur texte, >= 3:1 sur composants) */
  --bg-app: #f8fafc;
  --bg-card: #ffffff;
  --bg-subtle: #f1f5f9;
  --border-color: #94a3b8;
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
  :root {
    --bg-app: #0b0f17;
    --bg-card: #151d2c;
    --bg-subtle: #1e293b;
    --border-color: #64748b;
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
  border-radius: var(--radius-sm);
  z-index: 1000;
  font-weight: 600;
  text-decoration: none;
}
.skip-link:focus {
  top: 1rem;
  outline: 3px solid var(--focus-ring);
  outline-offset: 2px;
}

/* Conteneur principal */
.app-container {
  max-width: 1200px;
  margin: 0 auto;
  padding: 1.5rem 1rem 3rem 1rem;
}

@media (min-width: 640px) {
  .app-container {
    padding: 2rem 1.5rem 4rem 1.5rem;
  }
}

/* En-tête */
header.app-header {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding-bottom: 1.5rem;
  border-bottom: 1px solid var(--border-color);
  margin-bottom: 2rem;
}

@media (min-width: 768px) {
  header.app-header {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }
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
  padding: 0.2rem 0.5rem;
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
  background: currentColor;
}

/* Grille de statistiques */
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(1, 1fr);
  gap: 1rem;
  margin-bottom: 2rem;
}

@media (min-width: 640px) {
  .metrics-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (min-width: 1024px) {
  .metrics-grid {
    grid-template-columns: repeat(4, 1fr);
  }
}

.metric-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  box-shadow: var(--shadow-sm);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.metric-label {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.metric-value {
  font-size: 1.75rem;
  font-weight: 700;
  color: var(--text-main);
  line-height: 1;
}

.metric-desc {
  font-size: 0.8125rem;
  color: var(--text-dim);
}

/* Sections et cartes */
.section-panel {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: 1.5rem;
  margin-bottom: 2rem;
  box-shadow: var(--shadow-sm);
}

.section-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 1.25rem;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.section-title {
  font-size: 1.125rem;
  font-weight: 700;
  color: var(--text-main);
}

.section-subtitle {
  font-size: 0.875rem;
  color: var(--text-muted);
  margin-top: 0.25rem;
}

/* Tableaux modernes et accessibles */
.table-wrapper {
  overflow-x: auto;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
}

table.data-table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
  font-size: 0.875rem;
}

table.data-table th {
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-weight: 600;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--border-color);
  white-space: nowrap;
}

table.data-table td {
  padding: 0.875rem 1rem;
  border-bottom: 1px solid var(--border-color);
  color: var(--text-main);
  vertical-align: middle;
}

table.data-table tr:last-child td {
  border-bottom: none;
}

table.data-table tr:hover {
  background: var(--bg-subtle);
}

.code-cell {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
}

/* États vides honnêtes */
.empty-state {
  text-align: center;
  padding: 2.5rem 1.5rem;
  background: var(--bg-subtle);
  border-radius: var(--radius-md);
  border: 1px dashed var(--border-color);
}

.empty-state-title {
  font-weight: 600;
  font-size: 1rem;
  color: var(--text-main);
  margin-bottom: 0.35rem;
}

.empty-state-text {
  font-size: 0.875rem;
  color: var(--text-muted);
  max-width: 480px;
  margin: 0 auto;
}

/* Formulaires et boutons */
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

.form-input {
  font-family: var(--font-mono);
  font-size: 0.875rem;
  padding: 0.625rem 0.875rem;
  min-height: 44px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-main);
}

.form-input:focus {
  outline: 3px solid var(--focus-ring);
  outline-offset: 1px;
  border-color: var(--primary);
}

.form-help {
  font-size: 0.8125rem;
  color: var(--text-dim);
}

/* Badge de fonctionnalité future */
.badge-future {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 0.2rem 0.5rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  color: var(--text-muted);
}

.future-card {
  background: var(--bg-subtle);
  border: 1px dashed var(--border-color);
  border-radius: var(--radius-md);
  padding: 1.25rem;
}

/* Zone d'alerte / message en direct */
.toast-notice {
  padding: 0.75rem 1rem;
  border-radius: var(--radius-sm);
  font-size: 0.875rem;
  margin-bottom: 1rem;
  display: none;
}

.toast-notice.visible {
  display: block;
}

.toast-notice.info {
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  color: var(--text-main);
}

.toast-notice.success {
  background: var(--success-bg);
  border: 1px solid var(--success-border);
  color: var(--success-text);
}

.toast-notice.error {
  background: var(--danger-bg);
  border: 1px solid var(--danger-border);
  color: var(--danger-text);
}

/* Accessibilité pour mouvements réduits */
@media (prefers-reduced-motion: reduce) {
  * {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}
"#;
