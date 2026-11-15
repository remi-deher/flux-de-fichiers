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
  box-shadow: var(--shadow-sm);
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
  box-shadow: inset 0 2px 10px rgba(0, 0, 0, 0.14);
  min-height: 360px;
}

.flow-graph {
  position: relative;
}

.flow-graph__links {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.flow-graph__slot {
  position: absolute;
  border-radius: var(--radius-md);
}

.is-dragging {
  opacity: 0.5;
}

.flow-graph__slot.is-drop-ok {
  outline: 2px dashed var(--success);
  outline-offset: 3px;
}

.flow-graph__slot.is-drop-refused {
  outline: 2px dashed var(--danger);
  outline-offset: 3px;
}

.flow-graph__card {
  position: relative;
  box-sizing: border-box;
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 0.2rem;
  background: linear-gradient(180deg, var(--bg-elevated) 0%, var(--bg-card) 100%);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  padding: 0.6rem 0.75rem;
  text-align: left;
  color: var(--text-main);
  cursor: pointer;
  overflow: visible;
  transition: transform 0.16s var(--ease-out), box-shadow 0.16s var(--ease-out), border-color 0.16s var(--ease-out);
}

.flow-graph__card:hover {
  border-color: var(--border-highlight);
  transform: translateY(-1px);
  box-shadow: var(--shadow-md);
}

.flow-graph__card.is-selected {
  border-color: var(--primary);
  box-shadow: var(--shadow-glow);
}

.flow-graph__card.is-incomplete {
  border-color: var(--warning);
  box-shadow: 0 0 0 1px var(--warning-bg);
}

.flow-graph__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.4rem;
}

.flow-graph__kind {
  font-size: 0.6875rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  font-weight: 700;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.flow-graph__badge {
  flex: none;
  font-size: 0.6875rem;
  font-weight: 700;
  color: var(--warning);
}

.flow-graph__name {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flow-graph__port-dot {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--bg-card);
  border: 2px solid var(--primary);
  pointer-events: none;
}

.flow-graph__port-dot.is-in {
  top: -5px;
}

.flow-graph__port-dot.is-out {
  bottom: -5px;
}

.flow-graph__link-halo {
  fill: none;
  stroke: color-mix(in srgb, var(--primary) 16%, transparent);
  stroke-width: 7;
  stroke-linecap: round;
}

.flow-graph__link {
  fill: none;
  stroke: var(--primary);
  stroke-width: 2.5;
  stroke-linecap: round;
  transition: stroke 0.2s ease;
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
