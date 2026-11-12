//! Styles du kit de widgets : grille, coquille, états, listes, barres et progression.

pub const WIDGETS_CSS: &str = r#"
/* --- Grille de widgets : six colonnes, une seule sur petit écran --- */
.widget-grid {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: var(--space-3);
  margin-bottom: var(--space-4);
}

.widget {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  padding: var(--space-3);
  background-color: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
}

.widget--small { grid-column: span 2; }
.widget--medium { grid-column: span 3; }
.widget--large { grid-column: span 6; }

@media (max-width: 960px) {
  .widget--small,
  .widget--medium,
  .widget--large {
    grid-column: span 6;
  }
}

.widget__head {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.widget__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 2rem;
  height: 2rem;
  border-radius: var(--radius-sm);
  background-color: var(--primary-bg);
  color: var(--primary);
}

.widget__title {
  flex: 1;
  min-width: 0;
  font-size: 1rem;
  font-weight: 700;
  color: var(--text-main);
}

.widget__actions {
  flex: none;
}

.widget__body {
  min-width: 0;
}

.widget__message {
  margin: 0;
  font-size: 0.875rem;
  color: var(--text-muted);
}

/* États : le squelette remplace le contenu tant que les données n'ont pas répondu. */
.widget[data-state="loading"] .widget__body {
  min-height: 4.5rem;
  border-radius: var(--radius-sm);
  background: linear-gradient(90deg, var(--bg-subtle) 25%, var(--border-color) 50%, var(--bg-subtle) 75%);
  background-size: 200% 100%;
  animation: widget-shimmer 1.4s linear infinite;
}

.widget[data-state="loading"] .widget__message,
.widget[data-state="ready"] .widget__message {
  display: none;
}

.widget[data-state="empty"] .widget__body,
.widget[data-state="error"] .widget__body {
  display: none;
}

.widget[data-state="error"] .widget__message {
  color: var(--danger-text);
}

@keyframes widget-shimmer {
  from { background-position: 200% 0; }
  to { background-position: -200% 0; }
}

/* --- Listes de lignes (pastille, libellé, métadonnée) --- */
.widget-list {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  margin: 0;
  padding: 0;
  list-style: none;
}

.widget-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  width: 100%;
  padding: 0.5rem 0.65rem;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
}

button.widget-row {
  cursor: pointer;
}

button.widget-row:hover {
  background-color: var(--bg-subtle);
}

.widget-row__dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--text-muted);
}

.widget-row[data-tone="success"] .widget-row__dot { background-color: var(--success); }
.widget-row[data-tone="warning"] .widget-row__dot { background-color: var(--warning); }
.widget-row[data-tone="danger"] .widget-row__dot { background-color: var(--danger); }

.widget-row__label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  font-weight: 600;
  color: var(--text-main);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.widget-row__meta {
  flex: none;
  font-size: 0.75rem;
  color: var(--text-dim);
}

/* --- Progression native (accessible) --- */
.widget-progress {
  flex: none;
  width: 6rem;
  height: 0.5rem;
  accent-color: var(--primary);
}

/* --- Histogramme CSS : la hauteur de chaque barre vient de --bar-height --- */
.widget-bars {
  display: flex;
  align-items: flex-end;
  gap: 3px;
  height: 6rem;
}

.widget-bars__bar {
  flex: 1;
  height: var(--bar-height, 4%);
  min-height: 3px;
  border-radius: 2px;
  background-color: var(--text-muted);
}

.widget-bars__bar[data-tone="success"] { background-color: var(--success); }
.widget-bars__bar[data-tone="danger"] { background-color: var(--danger); }

.widget-bars__axis {
  display: flex;
  justify-content: space-between;
  margin-top: 0.35rem;
  font-size: 0.6875rem;
  color: var(--text-muted);
}
"#;
