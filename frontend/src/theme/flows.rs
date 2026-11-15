//! Styles du concepteur de flux visuel (Flows) en Rust typé.

pub const FLOWS_CSS: &str = r#"
/* --- Concepteur et Gestionnaire de Flux (Flows) --- */
.form-input--code {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  line-height: 1.5;
  resize: vertical;
}

.canvas-empty-hint {
  color: var(--text-dim);
  text-align: center;
  padding: 2rem 1rem;
}

.flow-card__pipeline {
  padding: 0.65rem 0.75rem;
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
}

.flow-pipeline-container {
  overflow-x: auto;
  padding: 0.75rem 0;
}

.flow-pipeline {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.45rem;
}

.pipeline-step-node {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.35rem 0.6rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--text-main);
}

.pipeline-step-node--more {
  background: var(--primary-bg);
  border-color: var(--border-highlight);
  color: var(--primary);
}

.pipeline-connector {
  color: var(--text-muted);
  font-size: 0.85rem;
  user-select: none;
}

.pipeline-step-node.in-place {
  border-color: var(--success);
  background: var(--success-bg);
}

.pipeline-step-node.working-file {
  border-color: var(--info);
  background: var(--info-bg);
}

.pipeline-arrow {
  color: var(--text-muted);
  font-size: 1rem;
}

/* --- Canvas Visuel No-Code (Zapier / FileFlows style) --- */
.canvas-header-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.875rem 1.25rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  border-bottom: none;
  box-shadow: var(--shadow-sm);
  position: sticky;
  top: 0;
  z-index: 10;
}

.canvas-header-bar__identity {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.canvas-header-bar__title {
  margin: 0;
  font-size: 1.25rem;
}

.canvas-workspace-layout {
  display: flex;
  min-height: calc(100vh - 220px);
  height: calc(100vh - 220px);
  max-height: 85vh;
  border: 1px solid var(--border-color);
  border-radius: 0 0 var(--radius-md) var(--radius-md);
  overflow: hidden;
  background: var(--bg-card);
  box-shadow: var(--shadow-sm);
}

.canvas-blocks-sidebar {
  width: 260px;
  background: var(--bg-subtle);
  border-right: 1px solid var(--border-color);
  padding: 1rem;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.sidebar-block-group {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.sidebar-group-title {
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  color: var(--text-dim);
  letter-spacing: 0.05em;
}

.draggable-block-item {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  padding: 0.625rem 0.75rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.2s ease;
  user-select: none;
}

.draggable-block-item:hover {
  border-color: var(--primary);
  background: var(--info-bg);
}

.draggable-block-item.addable .block-add-plus {
  margin-left: auto;
  font-weight: bold;
  font-size: 1.1rem;
  color: var(--primary);
}

.block-drag-icon {
  font-size: 1.25rem;
}

.block-drag-info {
  display: flex;
  flex-direction: column;
  font-size: 0.8125rem;
}

.block-drag-info strong {
  color: var(--text-main);
}

.block-drag-info span {
  font-size: 0.6875rem;
  color: var(--text-dim);
}

.canvas-board-wrapper {
  flex: 1;
  position: relative;
  background-color: var(--bg-app);
  background-image: radial-gradient(circle, var(--border-color) 1px, transparent 1px);
  background-size: 24px 24px;
  overflow-y: auto;
  padding: 2.5rem 1.5rem;
  display: flex;
  justify-content: center;
}

.canvas-connections-svg {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 1;
}

.canvas-nodes-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2.5rem;
  width: 100%;
  max-width: 520px;
  position: relative;
  z-index: 2;
}

.canvas-node {
  width: 100%;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 1rem;
  box-shadow: var(--shadow-sm);
  position: relative;
  transition: all 0.2s ease;
  cursor: pointer;
}

.canvas-node:hover,
.canvas-node.selected {
  border-color: var(--primary);
  box-shadow: 0 4px 16px rgba(88, 166, 255, 0.15);
}

.node-header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.node-icon {
  font-size: 1.5rem;
}

.node-title-box {
  flex: 1;
}

.node-kind {
  font-size: 0.6875rem;
  text-transform: uppercase;
  color: var(--text-dim);
  font-weight: 700;
  letter-spacing: 0.05em;
}

.node-name {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--text-main);
  margin: 0;
}

.node-summary {
  margin-top: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-color);
  font-size: 0.8125rem;
  color: var(--text-dim);
  line-height: 1.4;
}

/* Connecteurs de nœuds */
.node-connector {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--bg-card);
  border: 2px solid var(--primary);
}

.connector-in {
  top: -6px;
}

.connector-out {
  bottom: -6px;
}

/* Tiroir inspecteur latéral */
.canvas-inspector-drawer {
  width: 340px;
  background: var(--bg-card);
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  transition: width 0.2s ease, transform 0.2s ease;
}

.canvas-inspector-drawer.closed {
  width: 0;
  border-left: none;
  overflow: hidden;
}

.inspector-header {
  padding: 1rem 1.25rem;
  border-bottom: 1px solid var(--border-color);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.inspector-body {
  padding: 1.25rem;
  overflow-y: auto;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

/* Lignes de connexion SVG */
path.sim-base-line {
  stroke: var(--border-color);
  stroke-width: 2px;
  fill: none;
}

path.sim-pulse-line {
  stroke: var(--primary);
  stroke-width: 3px;
  stroke-dasharray: 6 6;
  fill: none;
  animation: pulseFlow 1s linear infinite;
}

@keyframes pulseFlow {
  from {
    stroke-dashoffset: 24;
  }
  to {
    stroke-dashoffset: 0;
  }
}

/* --- Barre d'outils compacte de l'atelier nodal --- */
.studio-toolbar {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  padding: 0.75rem 0.9rem;
  margin-bottom: 0.75rem;
  box-shadow: var(--shadow-sm);
}

.studio-toolbar__top {
  display: flex;
  align-items: flex-end;
  gap: 0.85rem;
  flex-wrap: wrap;
}

.studio-toolbar__meta {
  flex: 1 1 28rem;
  display: flex;
  align-items: flex-end;
  gap: 0.65rem;
  flex-wrap: wrap;
  min-width: 0;
}

.studio-toolbar__field {
  flex: 1 1 11rem;
  min-width: 0;
}

.studio-toolbar__field--mask {
  flex: 0 1 8.5rem;
}

.studio-toolbar__field--desc {
  flex: 1.5 1 14rem;
}

.studio-toolbar__field .form-label {
  font-size: 0.72rem;
  margin-bottom: 0.25rem;
}

.studio-toolbar__field .form-input {
  padding: 0.4rem 0.6rem;
  font-size: 0.8125rem;
}

.studio-toolbar__actions {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  flex-wrap: wrap;
  margin-left: auto;
}

.studio-toolbar__status {
  font-size: 0.75rem;
  color: var(--text-dim);
  white-space: nowrap;
}

.studio-toolbar__toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.8rem;
  color: var(--text-dim);
  cursor: pointer;
}
"#;
