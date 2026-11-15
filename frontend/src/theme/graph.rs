//! Styles du plan interactif nodal, du dock palette, des cartes, des liaisons et de la minimap.

pub const GRAPH_CSS: &str = r#"
/* --- Dock Palette latéral --- */
.flow-palette-dock {
  width: 300px;
  flex-shrink: 0;
  background: var(--bg-card);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  transition: width 0.2s var(--ease-out);
  z-index: 14;
}

.flow-palette-dock.is-closed {
  width: 0;
  border-right: none;
}

.flow-palette__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.75rem 0.85rem;
  border-bottom: 1px solid var(--border-subtle);
}

.flow-palette__title-wrap {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.flow-palette__title {
  margin: 0;
  font-size: 0.9rem;
  font-weight: 700;
  color: var(--text-main);
}

.flow-palette__badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1.4rem;
  height: 1.2rem;
  padding: 0 0.4rem;
  border-radius: var(--radius-pill);
  background: var(--bg-subtle);
  color: var(--text-dim);
  font-size: 0.72rem;
  font-weight: 700;
}

.flow-palette__header-actions {
  display: flex;
  align-items: center;
  gap: 0.25rem;
}

.header-icon-btn.is-active {
  background: var(--primary-bg);
  color: var(--primary);
  border-color: var(--border-highlight);
}

.flow-palette__insertion {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.6rem 0.85rem;
  background: var(--primary-bg);
  border-bottom: 1px solid var(--border-highlight);
  font-size: 0.78rem;
  color: var(--text-main);
}

.flow-palette__insertion-text {
  flex: 1;
  min-width: 0;
  font-weight: 600;
}

.flow-palette__search {
  position: relative;
  display: flex;
  align-items: center;
  padding: 0.65rem 0.85rem;
  border-bottom: 1px solid var(--border-subtle);
}

.flow-palette__search-icon {
  position: absolute;
  left: 1.45rem;
  color: var(--text-muted);
  pointer-events: none;
}

.flow-palette__search-input {
  width: 100%;
  padding-left: 1.85rem;
  padding-right: 1.85rem;
  font-size: 0.8125rem;
}

.flow-palette__search-clear {
  position: absolute;
  right: 1.35rem;
  border: 0;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  padding: 0.2rem;
  display: inline-flex;
  align-items: center;
}

.flow-palette__groups {
  flex: 1;
  overflow-y: auto;
  padding: 0.65rem 0.85rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.flow-palette__empty {
  margin: 1rem 0;
  font-size: 0.8125rem;
  color: var(--text-dim);
  text-align: center;
}

.flow-palette__group {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.flow-palette__summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 0.72rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-dim);
  cursor: pointer;
  padding: 0.25rem 0.15rem;
  user-select: none;
}

.flow-palette__count {
  padding: 0.05rem 0.4rem;
  border-radius: var(--radius-pill);
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-size: 0.68rem;
}

.flow-palette__list {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  padding-top: 0.3rem;
}

.flow-palette__drag {
  display: block;
}

.flow-palette__card {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.52rem 0.65rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  border-left: 3px solid var(--component-accent, var(--primary));
  background: var(--bg-elevated);
  color: var(--text-main);
  text-align: left;
  cursor: pointer;
  transition: transform 0.14s var(--ease-out), border-color 0.14s var(--ease-out), background-color 0.14s var(--ease-out);
}

.flow-palette__card:hover {
  border-color: var(--component-accent, var(--primary));
  background: var(--bg-subtle);
  transform: translateY(-1px);
}

.flow-palette__card-icon {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--component-accent, var(--primary)) 14%, transparent);
  color: var(--component-accent, var(--primary));
}

.flow-palette__card-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.flow-palette__card-title {
  font-size: 0.8125rem;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flow-palette__card-meta {
  font-size: 0.68rem;
  color: var(--text-muted);
}

.flow-palette__card-plus {
  font-size: 1rem;
  font-weight: 700;
  color: var(--text-muted);
}

/* --- Zone centrale du canevas 2D --- */
.flow-canvas-area {
  flex: 1;
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background-color: var(--bg-app);
}

.flow-canvas__toolbar {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  flex-wrap: wrap;
  padding: 0.5rem 0.85rem;
  background: var(--bg-glass);
  border-bottom: 1px solid var(--border-color);
  z-index: 12;
}

.flow-canvas__banner {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-main);
}

.flow-canvas__banner.is-error {
  color: var(--danger-text);
}

.flow-graph-board {
  flex: 1;
  overflow: auto;
  background-color: var(--bg-app);
  background-image: radial-gradient(circle, var(--border-color) 1px, transparent 1px);
  background-size: 24px 24px;
  cursor: grab;
  user-select: none;
  position: relative;
}

.flow-graph-board.is-panning {
  cursor: grabbing;
}

.flow-canvas__empty {
  margin: 3rem auto;
  max-width: 440px;
  text-align: center;
}

.flow-graph-sizer {
  position: relative;
}

.flow-graph-stage {
  position: relative;
}

.flow-canvas__wires {
  position: absolute;
  inset: 0;
  overflow: visible;
  pointer-events: none;
}

/* --- Cartes nodales 2D --- */
.canvas-node-slot {
  position: absolute;
  z-index: 2;
}

.canvas-node-slot.is-drop-ok .canvas-node {
  outline: 2px dashed var(--success);
  outline-offset: 4px;
}

.canvas-node-slot.is-drop-refused .canvas-node {
  outline: 2px dashed var(--danger);
  outline-offset: 4px;
}

.canvas-node {
  position: relative;
  box-sizing: border-box;
  width: 100%;
  min-height: 92px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  background: linear-gradient(180deg, var(--bg-elevated) 0%, var(--bg-card) 100%);
  border: 1px solid var(--border-color);
  border-left: 4px solid var(--node-accent, var(--primary));
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  padding: 0.6rem 0.7rem 0.75rem;
  color: var(--text-main);
  transition: box-shadow 0.16s var(--ease-out), border-color 0.16s var(--ease-out);
}

.canvas-node:hover {
  border-color: var(--border-highlight);
  box-shadow: var(--shadow-md);
}

.canvas-node.is-selected {
  border-color: var(--primary);
  box-shadow: var(--shadow-glow);
}

.canvas-node.is-incomplete {
  border-color: var(--warning);
}

.canvas-node.is-dragging {
  opacity: 0.9;
  box-shadow: var(--shadow-lg);
}

.canvas-node__heading {
  display: flex;
  align-items: flex-start;
  gap: 0.55rem;
}

.canvas-node__icon {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--node-accent, var(--primary)) 14%, transparent);
  color: var(--node-accent, var(--primary));
  cursor: grab;
  padding: 0;
  margin-top: 0.1rem;
}

.canvas-node__icon:active {
  cursor: grabbing;
}

.canvas-node__main {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  padding: 0;
  text-align: left;
  color: inherit;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 0.12rem;
}

.canvas-node__title-row {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.canvas-node__title {
  font-size: 0.875rem;
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.canvas-node__warn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--warning-bg);
  color: var(--warning);
  font-size: 0.7rem;
  font-weight: 800;
}

.canvas-node__subtitle {
  font-size: 0.69rem;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.canvas-node__summary {
  margin-top: 0.2rem;
  padding: 0.15rem 0.4rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 0.68rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.canvas-node__actions {
  display: flex;
  align-items: center;
  gap: 0.15rem;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.canvas-node:hover .canvas-node__actions,
.canvas-node:focus-within .canvas-node__actions,
.canvas-node.is-selected .canvas-node__actions {
  opacity: 1;
}

.canvas-node__action-btn {
  width: 26px;
  height: 26px;
}

.canvas-node__action-btn.is-danger:hover {
  color: var(--danger-text);
  border-color: color-mix(in srgb, var(--danger) 40%, transparent);
  background: var(--danger-bg);
}

/* --- Ports d'entrée/sortie & Bouton + d'insertion --- */
.canvas-port {
  display: grid;
  place-items: center;
  padding: 0;
  border-radius: var(--radius-pill);
  cursor: crosshair;
  z-index: 4;
}

.canvas-port.is-input {
  position: absolute;
  top: -7px;
  left: 50%;
  transform: translateX(-50%);
  width: 14px;
  height: 14px;
  background: var(--bg-card);
  border: 2px solid var(--node-accent, var(--primary));
  transition: transform 0.14s ease, border-color 0.14s ease, box-shadow 0.14s ease;
}

.canvas-port.is-input:hover,
.canvas-port.is-input.is-valid-target {
  border-color: var(--success);
  background: var(--success);
  transform: translateX(-50%) scale(1.25);
  box-shadow: 0 0 0 4px var(--success-bg);
}

.canvas-node__outputs {
  position: absolute;
  left: 8px;
  right: 8px;
  bottom: -12px;
  display: grid;
  grid-template-columns: repeat(var(--output-count, 1), minmax(0, 1fr));
  pointer-events: none;
}

.canvas-output-slot {
  --port-color: var(--primary);
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  pointer-events: auto;
}

.canvas-output-slot[data-tone="success"] {
  --port-color: var(--success);
}

.canvas-output-slot[data-tone="warning"] {
  --port-color: var(--warning);
}

.canvas-port.is-output {
  width: 22px;
  height: 22px;
  background: var(--bg-card);
  border: 1.5px solid var(--port-color);
  color: var(--port-color);
  font-size: 0.72rem;
  font-weight: 700;
  line-height: 1;
  box-shadow: var(--shadow-sm);
  transition: transform 0.14s ease, background-color 0.14s ease;
}

.canvas-port.is-output:hover {
  transform: scale(1.15);
  background: var(--bg-subtle);
}

.canvas-port__add {
  position: absolute;
  top: 24px;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-pill);
  border: 1px solid var(--border-highlight);
  background: var(--bg-card);
  color: var(--primary);
  font-size: 0.85rem;
  font-weight: 700;
  line-height: 1;
  display: grid;
  place-items: center;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.canvas-output-slot:hover .canvas-port__add,
.canvas-output-slot:focus-within .canvas-port__add,
.canvas-node.is-selected .canvas-port__add {
  opacity: 1;
  pointer-events: auto;
}

.canvas-port__add:hover {
  transform: scale(1.12);
  background: var(--primary);
  color: var(--primary-contrast);
}

/* --- Liaisons SVG interactives --- */
.flow-wire__hit {
  fill: none;
  stroke: transparent;
  stroke-width: 18px;
  pointer-events: stroke;
  cursor: pointer;
}

.flow-wire {
  fill: none;
  stroke: var(--primary);
  stroke-width: 2.5px;
  stroke-linecap: round;
  transition: opacity 0.16s ease, stroke-width 0.16s ease;
}

.flow-wire[data-tone="success"] {
  stroke: var(--success);
}

.flow-wire[data-tone="warning"] {
  stroke: var(--warning);
}

.flow-wire.is-highlighted {
  stroke-width: 3.5px;
  filter: drop-shadow(0 0 3px var(--primary));
}

.flow-wire.is-muted {
  opacity: 0.25;
}

.flow-wire--ghost {
  stroke: var(--primary);
  stroke-dasharray: 6 6;
  animation: pulseWireGhost 1s linear infinite;
}

@keyframes pulseWireGhost {
  from {
    stroke-dashoffset: 24;
  }
  to {
    stroke-dashoffset: 0;
  }
}

.flow-wire__badge rect {
  fill: var(--bg-card);
  stroke: var(--border-color);
  stroke-width: 1px;
}

.flow-wire__badge text {
  fill: var(--text-dim);
  font-size: 10px;
  font-weight: 700;
}

.flow-wire__badge[data-tone="success"] rect {
  stroke: color-mix(in srgb, var(--success) 55%, transparent);
}

.flow-wire__badge[data-tone="success"] text {
  fill: var(--success);
}

.flow-wire__badge[data-tone="warning"] rect {
  stroke: color-mix(in srgb, var(--warning) 55%, transparent);
}

.flow-wire__badge[data-tone="warning"] text {
  fill: var(--warning);
}

/* --- Panneau Minimap & Zoom flottant --- */
.flow-minimap-panel {
  position: absolute;
  right: 16px;
  bottom: 16px;
  width: 210px;
  padding: 0.5rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-glass);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  box-shadow: var(--shadow-md);
  z-index: 10;
}

.flow-minimap__tools {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.25rem;
  margin-bottom: 0.35rem;
}

.flow-minimap__zoom-reset {
  flex: 1;
  height: 28px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-main);
  font-size: 0.74rem;
  font-weight: 600;
  cursor: pointer;
}

.flow-minimap__svg {
  display: block;
  width: 100%;
  height: 88px;
  border-radius: var(--radius-sm);
  background: var(--bg-app);
  border: 1px solid var(--border-subtle);
}

.mini-wire {
  stroke: var(--border-color);
  stroke-width: 4;
}

.mini-node {
  fill: var(--text-muted);
  opacity: 0.7;
}

.mini-node.is-selected {
  fill: var(--primary);
  opacity: 1;
}

/* --- Tiroir Inspecteur latéral --- */
.flow-inspector-drawer {
  width: 340px;
  flex-shrink: 0;
  background: var(--bg-card);
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  min-height: 0;
  transition: width 0.2s var(--ease-out);
  z-index: 15;
}

.flow-inspector-drawer.is-closed {
  width: 0;
  border-left: none;
  overflow: hidden;
}

.flow-inspector__shell {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.flow-inspector__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.65rem;
  padding: 0.85rem 1rem;
  border-bottom: 1px solid var(--border-subtle);
}

.flow-inspector__identity {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  min-width: 0;
}

.flow-inspector__eyebrow {
  font-size: 0.7rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-muted);
}

.flow-inspector__badge {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.14rem 0.48rem;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--node-accent, var(--primary)) 14%, transparent);
  color: var(--node-accent, var(--primary));
  font-size: 0.7rem;
  font-weight: 700;
  width: fit-content;
}

.flow-inspector__title {
  margin: 0;
  font-size: 0.98rem;
  font-weight: 700;
  color: var(--text-main);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.flow-inspector__body {
  flex: 1;
  overflow-y: auto;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.flow-inspector__section {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-subtle);
}

.flow-inspector__section-title {
  margin: 0;
  font-size: 0.78rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-dim);
}

.flow-inspector__output-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  margin-bottom: 0.25rem;
}

.flow-inspector__footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--border-subtle);
  background: var(--bg-elevated);
}

/* Sélecteur historique conservé pour les tests de thème. */
.flow-graph__port-dot {
  position: absolute;
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

@media (max-width: 960px) {
  .flow-palette-dock {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: min(300px, 100%);
    box-shadow: var(--shadow-lg);
  }
  .flow-inspector-drawer {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    width: min(340px, 100%);
    box-shadow: var(--shadow-lg);
  }
}

@media (max-width: 680px) {
  .flow-minimap-panel {
    width: 168px;
    right: 10px;
    bottom: 10px;
  }
}
"#;
