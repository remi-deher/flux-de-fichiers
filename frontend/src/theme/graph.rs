//! Styles du plan interactif nodal, du dock palette, des cartes, des liaisons et de la minimap.

pub const GRAPH_CSS: &str = r#"
/* --- Tiroir droit unifié et flottant (Bloc / Simulation / Réglages) --- */
.flow-right-dock {
  position: absolute;
  top: 68px;
  right: 12px;
  bottom: 12px;
  width: 340px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-glass);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  transition: transform 0.2s var(--ease-out), opacity 0.2s var(--ease-out);
  z-index: 16;
}

.flow-right-dock.is-closed {
  transform: translateX(calc(100% + 18px));
  opacity: 0;
  pointer-events: none;
}

.flow-right-dock__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.4rem;
  padding: 0.45rem 0.55rem;
  border-bottom: 1px solid var(--border-subtle);
  background: var(--bg-card);
  flex-shrink: 0;
}

.flow-right-dock__tabs {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  min-width: 0;
  flex: 1;
}

.flow-right-dock__tab {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 0.5rem;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-dim);
  font-size: 0.76rem;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition: background-color 0.14s ease, color 0.14s ease, border-color 0.14s ease;
}

.flow-right-dock__tab:hover:not(:disabled) {
  color: var(--text-main);
  background: var(--bg-subtle);
}

.flow-right-dock__tab[aria-selected="true"] {
  background: var(--primary-bg);
  color: var(--primary);
  border-color: var(--border-highlight);
}

.flow-right-dock__tab:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.flow-right-dock__body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.flow-dock-pane {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.flow-palette__subbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.45rem 0.75rem 0;
}

/* --- Dock Palette flottant gauche --- */
.flow-palette-dock {
  position: absolute;
  top: 68px;
  left: 12px;
  bottom: 12px;
  width: 304px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-glass);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  transition: transform 0.2s var(--ease-out), opacity 0.2s var(--ease-out);
  z-index: 16;
}

.flow-palette-dock.is-closed {
  transform: translateX(calc(-100% - 20px));
  opacity: 0;
  pointer-events: none;
}

.flow-palette__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.55rem 0.75rem;
  border-bottom: 1px solid var(--border-subtle);
  background: var(--bg-card);
  flex-shrink: 0;
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
  cursor: grab;
  user-select: none;
  transition: transform 0.14s var(--ease-out), border-color 0.14s var(--ease-out), background-color 0.14s var(--ease-out);
}

.flow-palette__card:active {
  cursor: grabbing;
}

.flow-palette__card:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
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
  overflow: hidden;
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
  position: relative;
  z-index: 2;
  margin: 3rem auto;
  max-width: 440px;
  text-align: center;
}

.flow-graph-stage {
  position: absolute;
  left: 0;
  top: 0;
  width: 0;
  height: 0;
  z-index: 2;
  overflow: visible;
  transform-origin: 0 0;
}

.flow-canvas__wires {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 1;
  overflow: visible;
  pointer-events: none;
}

/* --- Cartes nodales 2D --- */
.canvas-node-slot {
  position: absolute;
  z-index: 2;
}

.canvas-node-slot:has(.canvas-node.is-dragging) {
  z-index: 6;
}

.canvas-drop-ghost {
  position: absolute;
  z-index: 3;
  box-sizing: border-box;
  pointer-events: none;
  border: 2px dashed var(--primary);
  border-radius: var(--radius-md);
  background: color-mix(in srgb, var(--primary) 10%, var(--bg-elevated));
  box-shadow: var(--shadow-md);
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 0.2rem;
  padding: 0.6rem 0.75rem;
}

.canvas-drop-ghost__badge {
  font-size: 0.68rem;
  font-weight: 700;
  color: var(--primary);
  letter-spacing: 0.02em;
}

.canvas-drop-ghost__title {
  font-size: 0.875rem;
  font-weight: 700;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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
  height: 104px;
  display: flex;
  flex-direction: column;
  background: linear-gradient(180deg, var(--bg-elevated) 0%, var(--bg-card) 100%);
  border: 1px solid var(--border-color);
  border-top: 3px solid var(--node-accent, var(--primary));
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  padding: 0;
  color: var(--text-main);
  cursor: grab;
  transition: box-shadow 0.16s var(--ease-out), border-color 0.16s var(--ease-out), transform 0.16s var(--ease-out);
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
  opacity: 0.92;
  border-color: var(--primary);
  box-shadow: var(--shadow-lg);
  cursor: grabbing;
}

.canvas-node__header {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  padding: 0.42rem 0.55rem;
  background: color-mix(in srgb, var(--node-accent, var(--primary)) 10%, var(--bg-elevated));
  border-bottom: 1px solid var(--border-subtle);
  border-radius: calc(var(--radius-md) - 2px) calc(var(--radius-md) - 2px) 0 0;
}

.canvas-node__icon {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, var(--node-accent, var(--primary)) 16%, transparent);
  color: var(--node-accent, var(--primary));
  padding: 0;
}

.canvas-node__meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.02rem;
  line-height: 1.15;
}

.canvas-node__domain {
  font-size: 0.62rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--node-accent, var(--primary));
}

.canvas-node__kind {
  font-size: 0.68rem;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.canvas-node__step {
  font-size: 0.66rem;
  font-weight: 700;
  color: var(--text-muted);
  padding: 0.08rem 0.34rem;
  border-radius: var(--radius-pill);
  background: var(--bg-card);
  border: 1px solid var(--border-subtle);
  flex-shrink: 0;
}

.canvas-node__body {
  padding: 0.5rem 0.65rem 0.8rem;
  display: flex;
  flex-direction: column;
  flex: 1;
}

.canvas-node__main {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  padding: 0;
  text-align: left;
  color: inherit;
  cursor: inherit;
  display: flex;
  flex-direction: column;
  gap: 0.28rem;
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

.canvas-node__chips {
  display: flex;
  flex-wrap: nowrap;
  gap: 0.25rem;
  overflow: hidden;
  min-width: 0;
}

.canvas-node__chip {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  min-width: 0;
  max-width: 100%;
  flex-shrink: 0;
  padding: 0.14rem 0.42rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  font-family: var(--font-mono);
  font-size: 0.67rem;
  line-height: 1.25;
}

.canvas-node__chip-key {
  color: var(--text-muted);
  flex-shrink: 0;
}

.canvas-node__chip-val {
  min-width: 0;
  color: var(--text-main);
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.canvas-node__actions {
  display: flex;
  align-items: center;
  gap: 0.12rem;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.canvas-node:hover .canvas-node__actions,
.canvas-node:focus-within .canvas-node__actions,
.canvas-node.is-selected .canvas-node__actions {
  opacity: 1;
}

.canvas-node__action-btn {
  width: 24px;
  height: 24px;
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
  top: -8px;
  left: 50%;
  transform: translateX(-50%);
  width: 15px;
  height: 15px;
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
  flex-direction: row;
  align-items: center;
  justify-content: center;
  pointer-events: auto;
}

.canvas-output-slot[data-tone="success"] {
  --port-color: var(--success);
}

.canvas-output-slot[data-tone="warning"] {
  --port-color: var(--warning);
}

.canvas-port.is-output {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 24px;
  height: 24px;
  padding: 0;
  background: var(--bg-card);
  border: 1.5px solid var(--port-color);
  color: var(--port-color);
  font-size: 0.72rem;
  font-weight: 700;
  line-height: 1;
  box-shadow: var(--shadow-sm);
  transition: transform 0.14s ease, background-color 0.14s ease;
}

.canvas-port.is-output.canvas-port--pill {
  padding: 0 0.55rem;
  gap: 0.28rem;
  font-size: 0.68rem;
}

.canvas-port__label {
  white-space: nowrap;
}

.canvas-port.is-output:hover {
  transform: scale(1.08);
  background: var(--bg-subtle);
}

.canvas-port__add {
  width: 0;
  height: 20px;
  margin-left: 0;
  padding: 0;
  border: 0 solid var(--border-highlight);
  border-radius: var(--radius-pill);
  background: var(--bg-card);
  color: var(--primary);
  font-size: 0.85rem;
  font-weight: 700;
  line-height: 1;
  display: grid;
  place-items: center;
  overflow: hidden;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  opacity: 0;
  pointer-events: none;
  flex-shrink: 0;
  transition: opacity 0.15s ease, width 0.15s ease, margin-left 0.15s ease, transform 0.15s ease;
}

.canvas-output-slot:hover .canvas-port__add,
.canvas-output-slot:focus-within .canvas-port__add,
.canvas-node.is-selected .canvas-port__add {
  width: 20px;
  margin-left: 0.25rem;
  border-width: 1px;
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

.flow-wire__glow {
  fill: none;
  stroke: color-mix(in srgb, var(--primary) 28%, transparent);
  stroke-width: 8px;
  stroke-linecap: round;
  pointer-events: none;
}

.flow-wire__glow[data-tone="success"] {
  stroke: color-mix(in srgb, var(--success) 28%, transparent);
}

.flow-wire__glow[data-tone="warning"] {
  stroke: color-mix(in srgb, var(--warning) 28%, transparent);
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

.flow-wire__flow {
  fill: none;
  stroke: var(--primary-contrast);
  stroke-width: 2px;
  stroke-linecap: round;
  stroke-dasharray: 4 10;
  opacity: 0.78;
  pointer-events: none;
  animation: pulseWireFlow 0.9s linear infinite;
}

@keyframes pulseWireFlow {
  from {
    stroke-dashoffset: 14;
  }
  to {
    stroke-dashoffset: 0;
  }
}

.flow-wire__endpoint {
  fill: var(--primary);
  stroke: var(--bg-card);
  stroke-width: 1.5px;
  pointer-events: none;
}

.flow-wire__endpoint[data-tone="success"] {
  fill: var(--success);
}

.flow-wire__endpoint[data-tone="warning"] {
  fill: var(--warning);
}

.flow-wire__endpoint.is-muted {
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

.flow-wire__insert-btn {
  pointer-events: all;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.flow-wire-group:hover .flow-wire__insert-btn,
.flow-wire-group.is-highlighted .flow-wire__insert-btn,
.flow-wire__insert-btn:focus-visible {
  opacity: 1;
}

.flow-wire__insert-circle {
  fill: var(--bg-card);
  stroke: var(--border-highlight);
  stroke-width: 1.5px;
  transition: fill 0.14s ease, stroke 0.14s ease;
}

.flow-wire__insert-plus {
  fill: var(--primary);
  font-size: 13px;
  font-weight: 700;
  pointer-events: none;
}

.flow-wire__insert-btn:hover .flow-wire__insert-circle,
.flow-wire__insert-btn:focus-visible .flow-wire__insert-circle {
  fill: var(--primary);
  stroke: var(--primary);
}

.flow-wire__insert-btn:hover .flow-wire__insert-plus,
.flow-wire__insert-btn:focus-visible .flow-wire__insert-plus {
  fill: var(--primary-contrast);
}

/* --- Barre verticale flottante bas-gauche (Zoom, Fit, Layout, Undo/Redo) --- */
.flow-canvas-controls {
  position: absolute;
  left: 14px;
  bottom: 14px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.2rem;
  padding: 0.3rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-glass);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  box-shadow: var(--shadow-md);
  transition: left 0.2s var(--ease-out);
  z-index: 11;
}

.flow-workspace-body.is-palette-open .flow-canvas-controls {
  left: 328px;
}

.flow-control-btn {
  width: 30px;
  height: 30px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-main);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 0.7rem;
  font-weight: 700;
  transition: background-color 0.14s ease, border-color 0.14s ease, color 0.14s ease;
}

.flow-control-btn:hover:not(:disabled) {
  background: var(--bg-subtle);
  border-color: var(--border-color);
}

.flow-control-btn.is-active {
  background: var(--primary-bg);
  border-color: color-mix(in srgb, var(--primary) 45%, transparent);
  color: var(--primary);
}

.flow-control-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.flow-control-btn--pct {
  height: 24px;
  font-size: 0.66rem;
  color: var(--text-dim);
}

.flow-controls-sep {
  width: 20px;
  height: 1px;
  background: var(--border-subtle);
  margin: 0.1rem 0;
}

/* --- Panneau Minimap flottant allégé --- */
.flow-minimap-panel {
  position: absolute;
  right: 14px;
  bottom: 14px;
  width: 168px;
  padding: 0.32rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-glass);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  box-shadow: var(--shadow-md);
  opacity: 0.84;
  transition: right 0.2s var(--ease-out), opacity 0.15s ease;
  z-index: 10;
}

.flow-minimap-panel:hover {
  opacity: 1;
}

.flow-workspace-body.is-panel-open .flow-minimap-panel {
  right: 364px;
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

.mini-viewport {
  fill: color-mix(in srgb, var(--primary) 14%, transparent);
  stroke: var(--primary);
  stroke-width: 1.5px;
  vector-effect: non-scaling-stroke;
  pointer-events: none;
}

.flow-quick-add {
  position: absolute;
  z-index: 19;
  width: 280px;
  max-height: 340px;
  display: flex;
  flex-direction: column;
  background: var(--bg-glass);
  backdrop-filter: blur(18px);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}

.flow-quick-add__search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border-color);
}

.flow-quick-add__search-icon {
  color: var(--text-muted);
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
}

.flow-quick-add__input {
  width: 100%;
  border: 0;
  background: transparent;
  color: var(--text-main);
  font-size: 13px;
  outline: none;
}

.flow-quick-add__input::placeholder {
  color: var(--text-muted);
}

.flow-quick-add__list {
  overflow-y: auto;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.flow-quick-add__group-label {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-muted);
  padding: 6px 8px 3px;
}

.flow-quick-add__item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-main);
  cursor: pointer;
  text-align: left;
}

.flow-quick-add__item:hover,
.flow-quick-add__item.is-active {
  background: var(--primary-bg);
  border-color: color-mix(in srgb, var(--primary) 35%, transparent);
}

.flow-quick-add__icon {
  color: var(--text-dim);
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
}

.flow-quick-add__text {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.flow-quick-add__label {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.flow-quick-add__cat {
  font-size: 11px;
  color: var(--text-muted);
}

.flow-quick-add__empty {
  padding: 14px 10px;
  text-align: center;
  font-size: 12px;
  color: var(--text-muted);
}

.flow-canvas__marquee {
  position: absolute;
  pointer-events: none;
  z-index: 6;
  border: 1.5px solid var(--primary);
  background: color-mix(in srgb, var(--primary) 14%, transparent);
  border-radius: var(--radius-sm);
}

.flow-wire__popover {
  position: absolute;
  z-index: 16;
  transform: translate(-50%, -50%);
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-color);
  background: var(--bg-glass);
  backdrop-filter: blur(14px);
  -webkit-backdrop-filter: blur(14px);
  box-shadow: var(--shadow-md);
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

/* Point central du port d'entrée (et sélecteur vérifié par les tests de thème). */
.flow-graph__port-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--node-accent, var(--primary));
  transition: background-color 0.14s ease;
}

.canvas-port.is-input:hover .flow-graph__port-dot,
.canvas-port.is-input.is-valid-target .flow-graph__port-dot {
  background: var(--primary-contrast);
}

/* --- Simulation interactive du parcours dans l'éditeur --- */
.canvas-node.is-sim-visited {
  border-color: var(--success);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--success) 24%, transparent);
}

.canvas-node.is-sim-active {
  border-color: var(--primary);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--primary) 35%, transparent);
}

.flow-wire.is-sim-path {
  stroke: var(--success);
  stroke-width: 3px;
  opacity: 1;
}

.flow-sim__presets {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.flow-sim__fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.5rem;
}

.flow-sim__controls {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.flow-sim__warnings {
  margin: 0;
  padding: 0.55rem 0.75rem 0.55rem 1.4rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--warning);
  background: color-mix(in srgb, var(--warning) 10%, transparent);
  color: var(--text-main);
  font-size: 0.8rem;
}

.flow-sim__steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.flow-sim__step-btn {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.45rem 0.55rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  background: var(--bg-card);
  color: var(--text-main);
  text-align: left;
  cursor: pointer;
}

.flow-sim__step-btn:hover {
  border-color: var(--border-highlight);
}

.flow-sim__step-btn.is-active {
  border-color: var(--primary);
  background: var(--primary-bg);
}

.flow-sim__step-num {
  color: var(--text-dim);
  font-size: 0.75rem;
}

.flow-sim__step-meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  font-size: 0.8rem;
}

@media (max-width: 960px) {
  .flow-right-dock {
    top: 0;
    right: 0;
    bottom: 0;
    width: min(340px, 100%);
    border-radius: 0;
    z-index: 25;
  }
  .flow-workspace-body.is-panel-open .flow-minimap-panel {
    right: 14px;
  }
  .flow-workspace-body.is-palette-open .flow-canvas-controls {
    left: 14px;
  }
  .flow-palette-dock {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: min(304px, 100%);
    border-radius: 0;
    box-shadow: var(--shadow-lg);
    z-index: 25;
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
    width: 140px;
    right: 10px;
    bottom: 10px;
  }
  .flow-canvas-controls {
    left: 10px;
    bottom: 10px;
  }
}
"#;
