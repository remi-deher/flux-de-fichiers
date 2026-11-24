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

.code-editor__stack {
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
