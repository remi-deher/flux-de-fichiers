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

.script-workbench__editor {
  flex: 1;
  min-height: 12rem;
  resize: none;
  white-space: pre;
  overflow: auto;
  tab-size: 2;
  line-height: 1.5;
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

  .script-workbench__editor {
    min-height: 18rem;
  }
}
"#;
