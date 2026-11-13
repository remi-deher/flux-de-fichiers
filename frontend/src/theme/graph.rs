//! Styles de l'éditeur de flux du client WASM (ADR 0014) : la géométrie (positions, tailles)
//! est calculée en Rust et posée en ligne ; ces règles ne portent que l'apparence.

pub const GRAPH_CSS: &str = r#"
/* --- Éditeur de flux (client WASM) --- */
.flow-editor {
  display: grid;
  grid-template-columns: minmax(180px, 220px) 1fr minmax(260px, 320px);
  gap: 1rem;
  align-items: start;
}

@media (max-width: 1100px) {
  .flow-editor {
    grid-template-columns: 1fr;
  }
}

.flow-palette,
.flow-inspector {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.flow-palette__item {
  text-align: left;
  width: 100%;
}

.flow-graph-board {
  overflow: auto;
  background-color: var(--bg-app);
  background-image: radial-gradient(circle, var(--border-color) 1px, transparent 1px);
  background-size: 24px 24px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  min-height: 320px;
}

.flow-graph {
  position: relative;
}

.flow-graph__links {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.flow-graph__card {
  position: absolute;
  box-sizing: border-box;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  padding: 0.6rem 0.75rem;
  text-align: left;
  color: var(--text-main);
  cursor: pointer;
  overflow: hidden;
}

.flow-graph__card.is-selected {
  border-color: var(--primary);
  box-shadow: 0 0 0 2px var(--primary-bg);
}

.flow-graph__card.is-incomplete {
  border-color: var(--warning);
}

.flow-graph__kind {
  font-size: 0.75rem;
  color: var(--text-dim);
}

.flow-graph__name {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flow-graph__link {
  fill: none;
  stroke: var(--primary);
  stroke-width: 2.5;
}

.flow-graph__link--true {
  stroke: var(--success);
}

.flow-graph__link--false {
  stroke: var(--warning);
}

.flow-graph__port-label {
  font-size: 11px;
  fill: var(--text-dim);
}

.flow-headers__row {
  display: grid;
  grid-template-columns: 1fr 1fr auto;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}
"#;
