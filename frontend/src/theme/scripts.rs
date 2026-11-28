//! Styles de l'éditeur de script en pleine page (ADR 0030) : code à gauche, propriétés à droite.

pub const SCRIPTS_CSS: &str = r#"
/* --- Éditeur de script (pleine page) --- */
.script-workbench__bar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.6rem 1rem;
  border-bottom: 1px solid var(--border-subtle);
  background: var(--bg-card);
}

.script-workbench__title {
  margin: 0;
  font-size: var(--font-size-lg, 1.1rem);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.script-workbench__spacer {
  flex: 1;
}

.script-workbench__body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(320px, 26rem);
  gap: 0;
}

.script-workbench__code {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 0.75rem 1rem;
  gap: 0.4rem;
}

/* Zone de code : numéros, couche colorée et saisie transparente partagent police, interligne et marges. */
.code-editor {
  flex: 1;
  min-height: 12rem;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  overflow: auto;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-md);
  background: var(--bg-subtle);
  font-family: var(--font-mono);
  font-size: var(--font-size-sm);
  line-height: 1.5;
  tab-size: 2;
}

.code-editor__gutter {
  margin: 0;
  padding: 0.5rem 0.6rem;
  list-style: none;
  text-align: right;
  color: var(--text-dim);
  user-select: none;
  border-right: 1px solid var(--border-subtle);
  position: sticky;
  left: 0;
  background: inherit;
}

/* Anomalies du serveur de langage : soulignement des jetons, numéro de ligne coloré, résumé sous la zone. */
.diag-error { text-decoration: underline wavy var(--danger); text-decoration-thickness: 1px; text-underline-offset: 0.2em; }
.diag-warning { text-decoration: underline wavy var(--warning); text-decoration-thickness: 1px; text-underline-offset: 0.2em; }
.diag-information { text-decoration: underline dotted var(--info); text-decoration-thickness: 1px; text-underline-offset: 0.2em; }
.diag-hint { text-decoration: underline dotted var(--text-dim); text-decoration-thickness: 1px; text-underline-offset: 0.2em; }
.code-editor__gutter .diag-error { color: var(--danger); font-weight: 600; }
.code-editor__gutter .diag-warning { color: var(--warning); font-weight: 600; }
.code-editor__gutter .diag-information { color: var(--info); }
.code-editor__gutter li { text-decoration: none; }

.code-editor__status {
  margin: 0;
  color: var(--text-dim);
  font-size: var(--font-size-sm);
}

.code-editor__stack {
  position: relative;
  display: grid;
  min-width: 0;
}

.code-editor__layer {
  grid-area: 1 / 1;
  margin: 0;
  padding: 0.5rem 0.75rem;
  border: 0;
  font: inherit;
  letter-spacing: normal;
  white-space: pre;
  tab-size: inherit;
}

.code-editor__highlight {
  pointer-events: none;
  color: var(--text-main);
}

.code-editor__input {
  min-width: 0;
  resize: none;
  overflow: hidden;
  background: transparent;
  color: transparent;
  caret-color: var(--text-main);
  outline: none;
}

.code-editor__input::selection {
  background: var(--primary-bg);
}

.code-editor__suggestions {
  position: absolute;
  top: calc(0.5rem + (var(--caret-line) + 1) * 1.5em);
  left: calc(0.75rem + var(--caret-col) * 1ch);
  z-index: 5;
  min-width: 14rem;
  margin: 0;
  padding: 0.2rem;
  list-style: none;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-md);
  background: var(--bg-card);
  color: var(--text-main);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
}

.code-editor__suggestion {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.15rem 0.5rem;
  border-radius: var(--radius-sm, 4px);
  cursor: pointer;
}

.code-editor__suggestion.is-selected {
  background: var(--primary-bg);
}

.code-editor__hover {
  position: absolute;
  top: calc(0.5rem + (var(--caret-line) + 1) * 1.5em);
  left: calc(0.75rem + var(--caret-col) * 1ch);
  z-index: 6;
  max-width: min(40rem, 90%);
  padding: 0.35rem 0.6rem;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-md);
  background: var(--bg-card);
  color: var(--text-main);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
  pointer-events: none;
  white-space: normal;
}

.code-editor__hover p {
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.code-editor__hover pre {
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--accent);
}

.code-editor__hover p + p,
.code-editor__hover p + pre,
.code-editor__hover pre + p,
.code-editor__hover pre + pre {
  margin-top: 0.35rem;
}

/* Règle de mesure : sa largeur donne celle d'un caractère de la police fixe, sans rien afficher. */
.code-editor__ruler {
  position: absolute;
  visibility: hidden;
  pointer-events: none;
  white-space: pre;
}

.code-editor__suggestion-kind {
  color: var(--text-dim);
}

.code-editor:focus-within {
  border-color: var(--primary);
}

.script-workbench__side {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow-y: auto;
  padding: 0.75rem 1rem;
  border-left: 1px solid var(--border-subtle);
  background: var(--bg-card);
}

.script-workbench__panel[hidden] {
  display: none;
}

/* --- Panneau « Essai » (ADR 0031) --- */
.script-trial__result {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-top: 1rem;
}

.script-trial__response {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 0.3rem 0.75rem;
  margin: 0;
  align-items: start;
}

.script-trial__response dt {
  font-family: var(--font-mono);
  color: var(--text-dim);
}

.script-trial__response dd {
  margin: 0;
}

.script-trial__details {
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--bg-subtle);
}

.script-trial__summary {
  padding: 0.3rem 0.6rem;
  cursor: pointer;
  font-family: var(--font-mono);
  font-size: var(--font-size-sm);
}

.script-trial__stream {
  margin: 0;
  padding: 0.4rem 0.6rem;
  max-height: 16rem;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: var(--font-size-sm);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.script-trial__response .script-trial__stream {
  padding: 0;
  max-height: 8rem;
}

@media (max-width: 960px) {
  .script-workbench__body {
    grid-template-columns: 1fr;
    grid-auto-rows: minmax(0, auto);
    overflow-y: auto;
  }

  .script-workbench__side {
    border-left: 0;
    border-top: 1px solid var(--border-subtle);
  }

  .code-editor {
    min-height: 18rem;
  }
}
"#;
