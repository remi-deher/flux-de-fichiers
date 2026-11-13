//! Plan de travail du concepteur : rendu du graphe édité, inspecteur et ajout de blocs.
//!
//! Le DOM n'est qu'une vue de `editorGraph` (voir `graph.rs`) : la définition enregistrée est
//! chargée à l'ouverture, et seul ce modèle est renvoyé à l'enregistrement.

pub const SCRIPT: &str = r###"
    const PORT_STYLES = {
      true: { label: '✓ Vrai', className: 'out-true', color: 'var(--success)', marker: 'arrow-true' },
      false: { label: '✗ Faux', className: 'out-false', color: 'var(--warning)', marker: 'arrow-false' },
      fallback: { label: '⚡ Repli', className: 'out-fallback', color: 'var(--accent)', marker: 'arrow-fallback' },
    };
    const DEFAULT_PORT_STYLE = { color: 'var(--primary)', marker: 'arrow-default' };
    const CONFIG_SUMMARY_MAX = 3;

    function portStyle(port) {
      return (port != null && PORT_STYLES[port]) || DEFAULT_PORT_STYLE;
    }

    function blockIcon(type) {
      const value = String(type || '');
      if (value === 'file-input') return '📄';
      if (value === 'condition') return '🔀';
      if (value === 'mkvpropedit') return '⚡';
      if (value === FLOW_OUTPUT_BLOCK) return '💾';
      if (value.startsWith('sonarr')) return '📡';
      if (value.startsWith('radarr')) return '🎬';
      if (value.includes('webhook')) return '🔔';
      if (value.includes('video')) return '🎞️';
      if (value.includes('audio')) return '🔊';
      if (value.includes('mp4')) return '📦';
      return '🧩';
    }

    // Résumé lisible des premiers réglages scalaires d'un bloc.
    function configSummary(config) {
      const entries = Object.entries(config || {})
        .filter(([, value]) => value === null || typeof value !== 'object')
        .slice(0, CONFIG_SUMMARY_MAX)
        .map(([key, value]) => `${key} : ${value}`);
      return entries.length ? entries.join(' • ') : 'Aucun réglage';
    }

    function nodeElement(nodeId) {
      return nodesContainer ? nodesContainer.querySelector(`.canvas-node[data-node-id="${cssAttr(nodeId)}"]`) : null;
    }

    function renderNode(node) {
      const div = document.createElement('div');
      div.className = 'canvas-node';
      div.id = node.id;
      div.setAttribute('data-node-id', node.id);
      div.setAttribute('data-block-type', node.type || '');
      const ports = outputPorts(editorGraph, node);
      const hasInput = incomingEdges(editorGraph, node.id).length > 0 || node.type !== 'file-input';
      const simplePorts = ports.length === 1 && ports[0] == null;
      const outputs = simplePorts
        ? '<div class="node-connector connector-out"></div>'
        : (ports.length ? `<div class="node-multi-outputs">${ports.map(port => {
            const style = portStyle(port);
            const label = style.label || escapeHtml(port ?? 'Suite');
            return `<div class="multi-output-port ${style.className ? `port-${port}` : ''}">
                <span class="port-label">${label}</span>
                <div class="node-connector connector-out ${style.className || ''}" data-port="${escapeHtml(port ?? '')}"></div>
              </div>`;
          }).join('')}</div>` : '');
      div.innerHTML = `
        ${hasInput ? '<div class="node-connector connector-in"></div>' : ''}
        ${simplePorts ? outputs : ''}
        <div class="node-header">
          <span class="node-icon">${blockIcon(node.type)}</span>
          <div class="node-title-box">
            <span class="node-kind">${escapeHtml(node.type || 'bloc')}</span>
            <h4 class="node-name">${escapeHtml(node.name || node.id)}</h4>
          </div>
          <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
        </div>
        <div class="node-summary" id="${escapeHtml(node.id)}-summary">${escapeHtml(configSummary(node.config))}</div>
        ${simplePorts ? '' : outputs}
      `;
      bindNodeEvents(div);
      return div;
    }

    // Redessine tout le graphe depuis le modèle.
    function renderGraph(selectedId) {
      if (!nodesContainer) return;
      nodesContainer.innerHTML = '';
      if (editorGraph.nodes.length === 0) {
        nodesContainer.innerHTML = '<p class="canvas-empty-hint">Aucun bloc : ajoutez-en depuis la palette.</p>';
        renderInspector(null);
      } else {
        editorGraph.nodes.forEach(node => nodesContainer.appendChild(renderNode(node)));
        selectNode(nodeElement(selectedId) || nodesContainer.querySelector('.canvas-node'));
      }
      setTimeout(drawCanvasLines, 30);
    }

    async function loadEditorFlow(flowId) {
      const cached = cachedFlows.find(flow => flow.id === flowId);
      if (cached) return cached;
      const res = await apiFetch(`/api/v1/flows/${encodeURIComponent(flowId)}`);
      return res.json();
    }

    function showEditor(flow, graph) {
      editorFlow = flow;
      editorGraph = graph;
      invalidConfigs.clear();
      if (libraryView) libraryView.style.display = 'none';
      if (canvasView) canvasView.style.display = 'block';
      syncCapsuleActions('flow-editor');
      const titleEl = el('canvas-flow-title');
      if (titleEl) titleEl.textContent = `Éditeur visuel : ${flow.name || flow.id || 'Nouveau flux'}`;
      resetSimulation();
      renderGraph(null);
    }

    // Ouvre un flux enregistré tel qu'il est stocké : blocs, liaisons et réglages.
    async function openCanvas(flowId) {
      try {
        const flow = await loadEditorFlow(flowId);
        showEditor(flow, parseFlowGraph(flow.definition));
      } catch (_) {
        showNotice('Impossible de charger ce flux.', 'error');
      }
    }

    function closeCanvas() {
      if (canvasView) canvasView.style.display = 'none';
      if (libraryView) libraryView.style.display = 'block';
      syncCapsuleActions('flows');
    }

    if (backToLibraryBtn) backToLibraryBtn.addEventListener('click', closeCanvas);

    const editFlowBtn = el('flows-fact-action');
    if (editFlowBtn) {
      editFlowBtn.addEventListener('click', () => {
        const flowId = editFlowBtn.getAttribute('data-flow');
        if (!flowId) {
          showNotice('Ouvrez un flux avant de l\'éditer.', 'info');
          return;
        }
        openCanvas(flowId);
      });
    }

    // Écoute du changement d'onglet via subnav
    document.addEventListener('flux:subnav-change', (e) => {
      if (e.detail && e.detail.targetId === 'subtab-flows-canvas') {
        if (canvasView) canvasView.style.display = 'block';
        if (libraryView) libraryView.style.display = 'none';
        setTimeout(drawCanvasLines, 60);
      } else if (e.detail && e.detail.targetId === 'subtab-flows-installed') {
        if (canvasView) canvasView.style.display = 'none';
        if (libraryView) libraryView.style.display = 'block';
      }
    });

    // 2. Assistant de configuration (Setup Wizard)
    const openWizardSubnavBtn = el('btn-subnav-open-wizard');
    const triggerWizard = () => {
      if (!wizardDialog) return;
      if (typeof wizardDialog.showModal === 'function') {
        wizardDialog.showModal();
      } else {
        wizardDialog.setAttribute('open', '');
      }
    };
    if (openWizardBtn) openWizardBtn.addEventListener('click', triggerWizard);
    if (openWizardSubnavBtn) openWizardSubnavBtn.addEventListener('click', triggerWizard);

    function closeWizard() {
      if (wizardDialog) {
        if (typeof wizardDialog.close === 'function') {
          wizardDialog.close();
        } else {
          wizardDialog.removeAttribute('open');
        }
      }
    }

    if (closeWizardX) closeWizardX.addEventListener('click', closeWizard);
    if (cancelWizardBtn) cancelWizardBtn.addEventListener('click', closeWizard);

    // Sélection du modèle dans le wizard
    document.querySelectorAll('.wizard-template-card').forEach(card => {
      card.addEventListener('click', () => {
        document.querySelectorAll('.wizard-template-card').forEach(c => c.classList.remove('active'));
        card.classList.add('active');
        currentSelectedTemplate = card.getAttribute('data-template') || 'smart-hevc';
      });
    });

    // Graphe de départ d'un modèle de l'assistant ; rien n'est enregistré avant « Enregistrer ».
    function templateGraph(template) {
      const graph = parseFlowGraph(null);
      const input = appendGraphNode(graph, 'file-input', 'Fichier source');
      if (template === 'smart-hevc') {
        const condition = appendGraphNode(graph, 'condition', 'Condition : déjà HEVC ?',
          { property: 'video.codec', operator: 'eq', target: 'hevc' });
        const headers = appendGraphNode(graph, 'mkvpropedit', 'Édition des en-têtes (sur place)');
        const encode = { id: nextNodeId(graph), type: 'video-encode', name: 'Réencodage vidéo', config: { codec: 'hevc' } };
        graph.nodes.push(encode);
        const output = { id: nextNodeId(graph), type: FLOW_OUTPUT_BLOCK, name: 'Destination finale', config: { mode: 'replace' } };
        graph.nodes.push(output);
        graph.connections = [
          { from: input.id, to: condition.id },
          { from: condition.id, to: headers.id, port: 'true' },
          { from: condition.id, to: encode.id, port: 'false' },
          { from: headers.id, to: output.id },
          { from: encode.id, to: output.id },
        ];
      } else if (template === 'hevc') {
        appendGraphNode(graph, 'video-encode', 'Réencodage vidéo', { codec: 'hevc' });
        appendGraphNode(graph, 'audio-encode', 'Réencodage audio', { codec: 'aac' });
        appendGraphNode(graph, 'remux-mkv', 'Remuxage Matroska');
      } else if (template === 'mkv-remux') {
        appendGraphNode(graph, 'remux-mkv', 'Remuxage Matroska');
      } else if (template === 'mp4-web') {
        appendGraphNode(graph, 'remux-mp4', 'Remuxage MP4', { faststart: true });
      }
      return graph;
    }

    if (generateWizardBtn) {
      generateWizardBtn.addEventListener('click', () => {
        const nameInput = el('wizard-flow-name');
        const flowTitle = nameInput && nameInput.value ? nameInput.value.trim() : 'Nouveau flux';
        closeWizard();
        showEditor({ id: null, name: flowTitle, description: null, file_mask: DEFAULT_FILE_MASK, is_default: false },
          templateGraph(currentSelectedTemplate));
        showNotice(`Flux « ${flowTitle} » prêt dans l'éditeur ; enregistrez-le pour le conserver.`, 'info');
      });
    }

    // 3. Tracé des liaisons du modèle
    function connectorCenter(connector, boardRect, board) {
      const rect = connector.getBoundingClientRect();
      return {
        x: rect.left + rect.width / 2 - boardRect.left + board.scrollLeft,
        y: rect.top + rect.height / 2 - boardRect.top + board.scrollTop,
      };
    }

    function outConnector(nodeEl, port) {
      if (port != null) {
        const ported = nodeEl.querySelector(`.connector-out[data-port="${cssAttr(port)}"]`);
        if (ported) return ported;
      }
      return nodeEl.querySelector('.connector-out');
    }

    function drawCanvasLines() {
      if (!svgLines || !nodesContainer) return;
      const board = nodesContainer.parentElement;
      const boardRect = board.getBoundingClientRect();
      const markers = [DEFAULT_PORT_STYLE, ...Object.values(PORT_STYLES)].map(style => `
          <marker id="${style.marker}" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="${style.color}" />
          </marker>`).join('');
      let pathsHtml = `<defs>${markers}</defs>`;

      editorGraph.connections.forEach(edge => {
        const fromEl = nodeElement(edge.from);
        const toEl = nodeElement(edge.to);
        const outConn = fromEl && outConnector(fromEl, edge.port);
        const inConn = toEl && toEl.querySelector('.connector-in');
        if (!outConn || !inConn) return;
        const start = connectorCenter(outConn, boardRect, board);
        const end = connectorCenter(inConn, boardRect, board);
        const style = portStyle(edge.port);
        const isPulse = activeSimulation != null && edge.port === activeSimulation;
        const pulseClass = isPulse ? ' class="sim-pulse-line"' : '';
        pathsHtml += `<path d="M ${start.x} ${start.y} C ${start.x} ${start.y + 18}, ${end.x} ${end.y - 18}, ${end.x} ${end.y}" stroke="${style.color}" stroke-width="${isPulse ? '3.5' : '2.5'}" fill="none" marker-end="url(#${style.marker})"${pulseClass} />`;
      });
      svgLines.innerHTML = pathsHtml;
    }

    window.addEventListener('resize', drawCanvasLines);

    // 4. Ajout d'un bloc : il rejoint le modèle, relié au dernier bloc sans suite.
    function addBlock(blockType, title) {
      const node = appendGraphNode(editorGraph, blockType, title);
      renderGraph(node.id);
    }

    // 5. Inspecteur : nom et réglages réels du bloc sélectionné.
    function renderInspector(node) {
      const titleEl = el('inspector-block-title');
      const toolEl = el('inspector-block-tool');
      const formContainer = el('inspector-form-container');
      if (titleEl) titleEl.textContent = node ? (node.name || node.id) : 'Paramètres du bloc';
      if (toolEl) toolEl.textContent = node ? (node.type || 'bloc') : '—';
      if (!formContainer) return;
      if (!node) {
        formContainer.innerHTML = '<p class="form-help">Sélectionnez un bloc pour voir ses réglages.</p>';
        return;
      }
      formContainer.innerHTML = `
        <div class="form-group">
          <label for="inspector-node-name" class="form-label">Nom du bloc</label>
          <input type="text" id="inspector-node-name" class="form-input" value="${escapeHtml(node.name || '')}" />
        </div>
        <div class="form-group">
          <label for="inspector-node-config" class="form-label">Réglages (JSON)</label>
          <textarea id="inspector-node-config" class="form-input form-input--code" rows="10" spellcheck="false">${escapeHtml(JSON.stringify(node.config ?? {}, null, 2))}</textarea>
          <p class="form-help">Valeurs transmises telles quelles au bloc. Un en-tête de webhook laissé vide garde sa valeur enregistrée.</p>
          <p class="form-error" id="inspector-node-config-error" role="alert"></p>
        </div>
      `;
      const nameInput = el('inspector-node-name');
      const configInput = el('inspector-node-config');
      const errorEl = el('inspector-node-config-error');
      if (nameInput) nameInput.addEventListener('input', () => {
        node.name = nameInput.value;
        const nodeEl = nodeElement(node.id);
        const nameEl = nodeEl && nodeEl.querySelector('.node-name');
        if (nameEl) nameEl.textContent = node.name || node.id;
        if (titleEl) titleEl.textContent = node.name || node.id;
      });
      if (configInput) configInput.addEventListener('input', () => {
        const parsed = parseNodeConfig(configInput.value);
        if (parsed.ok) {
          node.config = parsed.value;
          invalidConfigs.delete(node.id);
          if (errorEl) errorEl.textContent = '';
          const summary = el(`${node.id}-summary`);
          if (summary) summary.textContent = configSummary(node.config);
        } else {
          invalidConfigs.add(node.id);
          if (errorEl) errorEl.textContent = parsed.error;
        }
      });
    }

    function selectNode(nodeEl) {
      if (!nodeEl) return;
      document.querySelectorAll('.canvas-node').forEach(n => n.classList.remove('selected'));
      nodeEl.classList.add('selected');
      renderInspector(graphNode(editorGraph, nodeEl.getAttribute('data-node-id')));
    }

    function bindNodeEvents(nodeEl) {
      nodeEl.addEventListener('click', (e) => {
        if (!e.target.classList.contains('node-remove-btn')) selectNode(nodeEl);
      });
      const removeBtn = nodeEl.querySelector('.node-remove-btn');
      if (removeBtn) {
        removeBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          const nodeId = nodeEl.getAttribute('data-node-id');
          removeGraphNode(editorGraph, nodeId);
          invalidConfigs.delete(nodeId);
          renderGraph(null);
        });
      }
    }

    renderGraph(null);

    // 6. Ajout de blocs depuis la barre latérale
    document.querySelectorAll('.draggable-block-item.addable').forEach(item => {
      item.addEventListener('click', () => {
        const blockType = item.getAttribute('data-block-type') || 'remux-mkv';
        const title = item.querySelector('strong') ? item.querySelector('strong').textContent : 'Nouveau bloc';
        addBlock(blockType, title);
        showNotice(`Bloc « ${title} » ajouté au flux`, 'info');
      });
    });

    // 7. Banc d'essai de simulation interactive ADR 0008
"###;

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn editor_renders_the_stored_graph_and_never_hardcodes_nodes() {
        assert!(SCRIPT.contains("parseFlowGraph(flow.definition)"));
        assert!(SCRIPT.contains("editorGraph.connections.forEach"));
        assert!(!SCRIPT.contains("flow.smart.adr0008"));
        assert!(!SCRIPT.contains("node-cfg-"));
    }

    #[test]
    fn inspector_edits_the_real_block_config() {
        assert!(SCRIPT.contains("id=\"inspector-node-config\""));
        assert!(SCRIPT.contains("parseNodeConfig(configInput.value)"));
        assert!(SCRIPT.contains("invalidConfigs.add(node.id)"));
    }
}
