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

/* Paramètres : lignes « à surveiller » et visibilité conditionnelle par recherche. */
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

.settings-alert__icon {
  flex: none;
  display: inline-flex;
  align-items: center;
  color: var(--warning);
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

@keyframes statusPulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.45; transform: scale(1.25); }
}

.status-dot.pulse,
.status-dot--pulse {
  animation: statusPulse 1.8s ease-in-out infinite;
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
  border-radius: var(--radius-md);
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
  transition: background-color 0.14s ease;
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
  box-shadow: var(--shadow-lg);
  transform: translateX(100%);
  transition: transform 0.22s var(--ease-out);
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

.inspection-drawer__subnav,
.page-subnav {
  display: flex;
  border-bottom: 1px solid var(--border-color);
}

.inspection-drawer__tab,
.page-subnav__tab {
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

.page-subnav__tab:hover {
  color: var(--text-main);
}

.inspection-drawer__tab.is-active,
.page-subnav__tab.is-active {
  color: var(--text-main);
  border-bottom-color: var(--primary);
}

.inspection-drawer__tab:focus-visible,
.page-subnav__tab:focus-visible {
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
  position: relative;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  background: radial-gradient(120% 80% at 100% 0%, color-mix(in srgb, var(--tone, var(--primary)) 8%, transparent) 0%, transparent 60%), var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  padding: var(--space-3);
  box-shadow: var(--shadow-sm);
  transition: transform 0.2s var(--ease-out), box-shadow 0.2s var(--ease-out), border-color 0.2s var(--ease-out);
}

.metric-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
  border-color: var(--border-highlight);
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

.metric-meter {
  width: 100%;
  height: 5px;
  margin-top: 0.25rem;
  border-radius: var(--radius-pill);
  background-color: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  overflow: hidden;
}

.metric-meter__fill {
  height: 100%;
  border-radius: var(--radius-pill);
  background-color: var(--tone, var(--primary));
  transition: width 0.35s var(--ease-out);
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

.detail-grid:has(> .detail-row) {
  display: flex;
  flex-direction: column;
  gap: 0;
}

.detail-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.55rem 0;
  border-bottom: 1px solid var(--border-subtle);
}

.detail-row:last-child {
  border-bottom: none;
}

@media (max-width: 640px) {
  .detail-grid {
    grid-template-columns: 1fr;
    gap: 0.25rem;
  }

  .detail-row {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.15rem;
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

.detail-row .detail-val {
  text-align: right;
  word-break: break-word;
}

@media (max-width: 640px) {
  .detail-row .detail-val {
    text-align: left;
  }
}

.stack-form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
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
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}
.empty-state-icon {
  width: 48px;
  height: 48px;
  margin-bottom: 0.75rem;
  border-radius: var(--radius-md);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  color: var(--primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.empty-state-title { margin: 0 0 0.5rem 0; font-size: 1.125rem; font-weight: 600; color: var(--text-main); }
.empty-state-message { margin: 0 auto 1.25rem auto; max-width: 500px; font-size: 0.875rem; color: var(--text-muted); }
.empty-state__action { display: flex; justify-content: center; gap: 0.5rem; }
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

/* --- Barre de progression temps réel (tableaux et fiches) --- */
.live-progress {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 112px;
}

.live-progress__track {
  flex: 1;
  min-width: 64px;
  height: 6px;
  border-radius: var(--radius-pill);
  background-color: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  overflow: hidden;
}

.live-progress__bar {
  height: 100%;
  border-radius: var(--radius-pill);
  background: linear-gradient(90deg, var(--primary), var(--accent));
  transition: width 0.28s var(--ease-out);
}

.live-progress__bar.is-indeterminate {
  width: 40%;
  animation: liveProgressSlide 1.4s ease-in-out infinite;
}

.live-progress[data-tone="success"] .live-progress__bar { background: var(--success); }
.live-progress[data-tone="warning"] .live-progress__bar { background: var(--warning); }
.live-progress[data-tone="danger"] .live-progress__bar { background: var(--danger); }

.live-progress__label {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-dim);
  min-width: 2.5rem;
  text-align: right;
}

@keyframes liveProgressSlide {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(260%); }
}

/* --- Pile de notifications Toast flottantes --- */
.toast-stack {
  position: fixed;
  right: max(1.25rem, var(--sar));
  bottom: max(1.25rem, var(--sab));
  z-index: 90;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-width: min(92vw, 380px);
  pointer-events: none;
}

.toast-item {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding: 0.75rem 0.9rem;
  border-radius: var(--radius-md);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  box-shadow: var(--shadow-lg);
  color: var(--text-main);
  font-size: 0.8125rem;
  line-height: 1.4;
  animation: toastSlideIn 0.22s var(--ease-out);
}

.toast-item--info { border-left: 3px solid var(--info); }
.toast-item--success { border-left: 3px solid var(--success); }
.toast-item--danger { border-left: 3px solid var(--danger); }

.toast-item__icon {
  flex: none;
  display: inline-flex;
  color: var(--info);
}

.toast-item--success .toast-item__icon { color: var(--success); }
.toast-item--danger .toast-item__icon { color: var(--danger); }

.toast-item__message {
  flex: 1;
  min-width: 0;
}

.toast-item__close {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.toast-item__close:hover {
  background: var(--bg-subtle);
  color: var(--text-main);
}

@keyframes toastSlideIn {
  from { opacity: 0; transform: translateY(10px) scale(0.98); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}

/* --- Palette de commandes clavier (Ctrl+K) --- */
.cmd-palette {
  position: fixed;
  inset: 0;
  z-index: 95;
  display: grid;
  place-items: start center;
  padding: 12vh 1rem 1rem;
}

.cmd-palette[hidden] {
  display: none;
}

.cmd-palette__scrim {
  position: fixed;
  inset: 0;
  border: 0;
  padding: 0;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.cmd-palette__dialog {
  position: relative;
  z-index: 1;
  width: min(92vw, 560px);
  background: var(--bg-card);
  border: 1px solid var(--border-highlight);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  animation: cmdPaletteIn 0.16s var(--ease-out);
}

@keyframes cmdPaletteIn {
  from { opacity: 0; transform: translateY(-8px) scale(0.98); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}

.cmd-palette__header {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding: 0.85rem 1rem;
  border-bottom: 1px solid var(--border-color);
  color: var(--text-dim);
}

.cmd-palette__input {
  flex: 1;
  min-width: 0;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--text-main);
  font: inherit;
  font-size: 0.9375rem;
}

.cmd-palette__input::placeholder {
  color: var(--text-muted);
}

.cmd-palette__list {
  max-height: 52vh;
  overflow-y: auto;
  padding: 0.4rem;
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.cmd-palette__empty {
  padding: 1.5rem 1rem;
  text-align: center;
  color: var(--text-muted);
  font-size: 0.8125rem;
}

.cmd-palette__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  width: 100%;
  padding: 0.55rem 0.75rem;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-main);
  font: inherit;
  font-size: 0.8125rem;
  text-align: left;
  cursor: pointer;
  transition: background-color 0.12s ease, border-color 0.12s ease;
}

.cmd-palette__item:hover,
.cmd-palette__item.is-active {
  background: var(--primary-bg);
  border-color: var(--border-highlight);
}

.cmd-palette__item-main {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-width: 0;
}

.cmd-palette__label {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cmd-palette__hint {
  color: var(--text-dim);
  font-size: 0.75rem;
  white-space: nowrap;
}

.cmd-palette__badge {
  flex: none;
  padding: 0.1rem 0.45rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  color: var(--text-dim);
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.cmd-palette__footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem 1rem;
  border-top: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-size: 0.75rem;
}

/* --- Sections ouvertes et tuiles de ressources unifiées --- */
.page-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-bottom: 1.75rem;
}

.resource-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(21rem, 1fr));
  gap: 1rem;
  margin-top: 0.25rem;
}

.resource-grid--list {
  grid-template-columns: 1fr;
}

.resource-card {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1.05rem 1.15rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--border-highlight);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  transition: border-color 0.16s ease, box-shadow 0.16s ease;
}

.resource-card[data-tone="success"] {
  border-left-color: var(--success);
}

.resource-card[data-tone="warning"] {
  border-left-color: var(--warning);
}

.resource-card[data-tone="danger"] {
  border-left-color: var(--danger);
}

.resource-card.is-clickable {
  cursor: pointer;
}

.resource-card:hover,
.resource-card.is-open {
  border-color: var(--primary);
  box-shadow: var(--shadow-md);
}

.resource-card__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.75rem;
}

.resource-card__icon {
  width: 34px;
  height: 34px;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  color: var(--primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.resource-card__titles {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  min-width: 0;
  flex: 1;
}

.resource-card__title {
  margin: 0;
  font-size: 0.98rem;
  font-weight: 700;
  color: var(--text-main);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.resource-card__subtitle {
  font-size: 0.78rem;
  color: var(--text-dim);
  line-height: 1.35;
}

.resource-card__badge {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  flex-shrink: 0;
}

.resource-card__body {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  flex: 1;
}

.resource-card__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding-top: 0.75rem;
  margin-top: auto;
  border-top: 1px solid var(--border-subtle);
}

.resource-card__meta {
  font-size: 0.75rem;
  color: var(--text-dim);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.resource-card__actions {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  flex-shrink: 0;
}

/* --- Étiquette unifiée (`Tag`) --- */
.ui-tag {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  max-width: 100%;
  padding: 0.2rem 0.5rem;
  border-radius: var(--radius-sm);
  font-size: 0.72rem;
  font-weight: 600;
  line-height: 1.25;
  border: 1px solid var(--border-subtle);
  background: var(--bg-subtle);
  color: var(--text-dim);
}

.ui-tag[data-tone="primary"] {
  background: var(--primary-bg);
  border-color: color-mix(in srgb, var(--primary) 35%, transparent);
  color: var(--primary);
}

.ui-tag[data-tone="success"] {
  background: var(--success-bg);
  border-color: color-mix(in srgb, var(--success) 35%, transparent);
  color: var(--success);
}

.ui-tag[data-tone="warning"] {
  background: var(--warning-bg);
  border-color: color-mix(in srgb, var(--warning) 35%, transparent);
  color: var(--warning);
}

.ui-tag[data-tone="danger"] {
  background: var(--danger-bg);
  border-color: color-mix(in srgb, var(--danger) 35%, transparent);
  color: var(--danger);
}

.ui-tag[data-tone="mono"] {
  font-family: var(--font-mono);
  font-weight: 500;
  color: var(--text-main);
}

.code-badge {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  padding: 0.15rem 0.45rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  color: var(--text-main);
  max-width: 100%;
}

/* --- Inspection & Diagnostic --- */
.inspection-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.65rem 0.85rem;
  border-radius: var(--radius-md);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  margin-bottom: 1rem;
}

.diagnostic-box {
  display: flex;
  align-items: flex-start;
  gap: 0.65rem;
  padding: 0.75rem 0.9rem;
  border-radius: var(--radius-md);
  background: var(--danger-bg);
  border: 1px solid var(--danger);
  color: var(--danger-text);
  margin-bottom: 1rem;
}

.diagnostic-box__title {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.diagnostic-box__message {
  font-size: 0.85rem;
  margin-top: 0.15rem;
}

.registre-row.is-clickable {
  cursor: pointer;
  transition: background-color 0.14s ease;
}

.registre-row.is-clickable:hover {
  background: var(--bg-subtle);
}

.system-volume-meter {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-top: 1rem;
  padding-top: 0.85rem;
  border-top: 1px solid var(--border-color);
}

/* --- Grille Bento des Paramètres --- */
.settings-bento-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1.25rem;
  margin-bottom: 1.5rem;
}

.settings-bento-grid > .section-panel {
  margin-bottom: 0;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

@media (max-width: 960px) {
  .settings-bento-grid {
    grid-template-columns: 1fr;
  }
}

.settings-help-details {
  margin-top: auto;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  overflow: hidden;
}

.settings-help-details__summary {
  padding: 0.6rem 0.85rem;
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-main);
  cursor: pointer;
  user-select: none;
}

.settings-help-details__summary:hover {
  color: var(--primary);
}

.settings-help-details__body {
  padding: 0.25rem 0.85rem 0.75rem;
  border-top: 1px solid var(--border-subtle);
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

/* --- Packs déclaratifs & étapes d'exécution --- */
.pack-components {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  margin-top: 0.5rem;
}

.step-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.step-item {
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  padding: 0.65rem 0.8rem;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.step-item__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.45rem;
}

.step-item__meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.step-item__msg {
  margin: 0;
  color: var(--text-main);
}

.step-excerpt {
  margin: 0.25rem 0 0;
  padding: 0.5rem 0.65rem;
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  border: 1px solid var(--border-subtle);
  color: var(--text-dim);
  font-size: 0.75rem;
  overflow-x: auto;
  max-height: 10rem;
  white-space: pre-wrap;
  word-break: break-word;
}

.jobs-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 0.75rem;
}

.jobs-toolbar .form-label {
  margin-bottom: 0;
}

.jobs-library-select {
  width: auto;
  min-width: 14rem;
}

.settings-form-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  gap: 0.75rem;
}

.settings-fieldset {
  margin: 0;
  padding: 0.85rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-subtle);
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}

.settings-fieldset__legend {
  padding: 0 0.35rem;
  font-size: 0.8125rem;
  font-weight: 700;
  color: var(--text-main);
}

.worker-card-telemetry {
  margin-top: 0.35rem;
}

.worker-telemetry-card {
  padding-top: 0.35rem;
  border-top: 1px solid var(--border-subtle);
}

.worker-telemetry-bars {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.worker-telemetry-metric {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.worker-telemetry-metric__head {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  font-size: 0.8125rem;
}

.inspection-drawer__panel:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
}

/* --- Sélecteur d'arborescence de bibliothèques --- */
.library-worker-row__controls {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex-wrap: wrap;
}

.library-worker-access-select {
  width: auto;
  min-width: 10rem;
  padding: 0.3rem 0.55rem;
  font-size: 0.75rem;
}

.library-worker-editor {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--border-subtle);
}

.library-access-warning {
  color: var(--warning);
}

.folder-picker {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  padding: 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  background: var(--bg-elevated);
}

.folder-picker__roots {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.folder-picker__root-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.3rem 0.65rem;
  border-radius: var(--radius-pill);
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-main);
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  transition: border-color 0.14s ease, background-color 0.14s ease;
}

.folder-picker__root-btn:hover {
  border-color: var(--primary);
}

.folder-picker__root-btn.is-active {
  background: var(--primary-bg);
  border-color: var(--primary);
  color: var(--primary);
}

.folder-picker__ro-pill {
  font-size: 0.65rem;
  font-weight: 700;
  padding: 0.05rem 0.3rem;
  border-radius: var(--radius-sm);
  background: var(--warning-bg);
  color: var(--warning);
}

.folder-picker__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  flex-wrap: wrap;
  padding: 0.35rem 0.5rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
}

.folder-picker__breadcrumbs {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.2rem;
  min-width: 0;
  flex: 1;
}

.folder-picker__crumb-sep {
  color: var(--text-muted);
  font-size: 0.75rem;
}

.folder-picker__crumb {
  border: 0;
  background: transparent;
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 0.75rem;
  padding: 0.15rem 0.35rem;
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.folder-picker__crumb:hover {
  background: var(--bg-card);
  color: var(--text-main);
}

.folder-picker__crumb.is-current {
  color: var(--text-main);
  font-weight: 700;
}

.folder-picker__toolbar-actions {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.folder-picker__error {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.65rem;
  border-radius: var(--radius-sm);
  background: var(--warning-bg);
  border: 1px solid var(--warning);
  color: var(--warning);
  font-size: 0.8125rem;
}

.folder-picker__error-text {
  flex: 1;
  min-width: 0;
}

.folder-picker__tree {
  display: flex;
  flex-direction: column;
  max-height: 18rem;
  overflow-y: auto;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  background: var(--bg-card);
}

.folder-picker__empty {
  padding: 0.9rem;
  text-align: center;
  color: var(--text-dim);
  font-size: 0.8125rem;
}

.folder-picker__row {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.25rem 0.5rem;
  padding-left: calc(0.45rem + var(--tree-depth, 0) * 1.1rem);
  border-bottom: 1px solid var(--border-subtle);
  transition: background-color 0.12s ease;
}

.folder-picker__row:last-child {
  border-bottom: none;
}

.folder-picker__row:hover {
  background: var(--bg-subtle);
}

.folder-picker__row.is-selected {
  background: var(--primary-bg);
}

.folder-picker__chevron {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.4rem;
  height: 1.4rem;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  transition: transform 0.15s var(--ease-out), color 0.12s ease;
}

.folder-picker__chevron:hover {
  background: var(--bg-subtle);
  color: var(--text-main);
}

.folder-picker__row.is-expanded .folder-picker__chevron svg {
  transform: rotate(90deg);
}

.folder-picker__chevron.is-hidden {
  visibility: hidden;
  pointer-events: none;
}

.folder-picker__item-btn {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.45rem;
  border: 0;
  background: transparent;
  color: var(--text-main);
  text-align: left;
  padding: 0.25rem 0.2rem;
  cursor: pointer;
  font-size: 0.8125rem;
}

.folder-picker__icon {
  flex: none;
  display: inline-flex;
  align-items: center;
  color: var(--primary);
}

.folder-picker__icon--file {
  color: var(--text-dim);
}

.folder-picker__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}

.folder-picker__file {
  padding-top: 0.35rem;
  padding-bottom: 0.35rem;
  font-size: 0.8125rem;
  color: var(--text-dim);
}

.folder-picker__meta {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  flex: none;
}

.folder-picker__badge {
  font-size: 0.7rem;
  font-weight: 600;
  padding: 0.1rem 0.4rem;
  border-radius: var(--radius-pill);
  background: var(--bg-subtle);
  color: var(--text-dim);
  border: 1px solid var(--border-subtle);
}

.folder-picker__badge--media {
  background: var(--info-bg);
  color: var(--info);
  border-color: transparent;
}

.folder-picker__badge--ro {
  background: var(--warning-bg);
  color: var(--warning);
  border-color: transparent;
}

.folder-picker__ext {
  font-family: var(--font-mono);
  font-size: 0.7rem;
  text-transform: uppercase;
  padding: 0.08rem 0.35rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  color: var(--text-main);
}

.folder-picker__size {
  font-family: var(--font-mono);
  font-size: 0.72rem;
  color: var(--text-muted);
}
"#;
