pub const SCRIPT: &str = r###"
  // --- Concepteur et Gestionnaire de Flux (Flows) type Zapier / FileFlows ---
  let cachedFlows = [];

  async function loadFlows() {
    const tbody = el('flows-table-body');
    try {
      const res = await apiFetch('/api/v1/flows');
      const data = await res.json();
      cachedFlows = Array.isArray(data) ? data : [];
      renderFlowsGrid();
    } catch {
      if (tbody && cachedFlows.length === 0) renderFlowsGrid();
    }
  }

  function flowBlockCount(flow) {
    try {
      const def = typeof flow.definition === 'string' ? JSON.parse(flow.definition) : flow.definition;
      const nodes = def && Array.isArray(def.nodes) ? def.nodes : [];
      return nodes.length;
    } catch {
      return 0;
    }
  }

  function selectFlowRow(flow) {
    const tbody = el('flows-table-body');
    if (!tbody || !flow) return;
    const row = tbody.querySelector(`tr[data-flow-id="${cssAttr(flow.id)}"]`);
    markRegistreRow(tbody, row);
    const count = flowBlockCount(flow);
    showRegistreFact('flows-fact', flow.name || flow.id, [
      flow.file_mask || 'Filtre automatique',
      count ? `${count} blocs` : 'Pipeline vide'
    ]);
    const action = el('flows-fact-action');
    if (action) {
      action.setAttribute('data-flow', flow.id || '');
      action.setAttribute('data-flow-title', flow.name || flow.id || 'Flux');
    }
  }

  function renderFlowsGrid() {
    const tbody = el('flows-table-body');
    if (!tbody) return;

    if (cachedFlows.length === 0) {
      tbody.innerHTML = emptyStateRow(4, 'Aucun flux installé. Importez-en un depuis le catalogue ou créez-en un.', false);
      const fact = el('flows-fact');
      if (fact) fact.hidden = true;
      return;
    }

    tbody.innerHTML = cachedFlows.map(flow => {
      const safeId = escapeHtml(flow.id);
      const safeName = escapeHtml(flow.name);
      const flowType = isVideoFlowName(flow.name) ? 'video' : 'other';
      const mask = escapeHtml(flow.file_mask || 'Filtre automatique');
      const count = flowBlockCount(flow);
      const composition = count ? `${count} blocs` : 'Pipeline vide';
      const badge = flow.is_default ? ' <span class="status-pill success">Défaut</span>' : '';
      return `<tr class="registre-row" data-flow-id="${safeId}" data-flow-type="${flowType}">
          <td class="text-strong">${safeName}${badge}</td>
          <td>${mask}</td>
          <td>${composition}</td>
          <td class="cell-end">
            <button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-flow="${safeId}">Ouvrir</button>
          </td>
      </tr>`;
    }).join('');

    tbody.querySelectorAll('.registre-open-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const flowId = btn.getAttribute('data-flow');
        const flow = cachedFlows.find(item => item.id === flowId);
        if (flow) selectFlowRow(flow);
      });
    });
  }

  function setupFlowsPanel() {
    const libraryView = el('flows-library-view');
    const canvasView = el('flows-canvas-view');
    const backToLibraryBtn = el('btn-back-to-library');
    const openWizardBtn = el('btn-open-flow-wizard');
    const wizardDialog = el('flow-wizard-dialog');
    const cancelWizardBtn = el('wizard-dialog-cancel-btn');
    const closeWizardX = el('wizard-dialog-close-btn');
    const generateWizardBtn = el('wizard-generate-flow-btn');
    const svgLines = el('canvas-svg-lines');
    const nodesContainer = el('canvas-nodes-container');

    let currentSelectedTemplate = 'smart-hevc';
    let activeSimulation = null; // null | 'true' | 'false' | 'fallback'
    let activeCanvasFlowId = 'flow.default.mkv';

    // 1. Navigation entre le registre et l'éditeur dédié
"###;
