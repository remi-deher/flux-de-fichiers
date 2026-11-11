  // --- Concepteur et Gestionnaire de Flux (Flows) type Zapier / FileFlows ---
  let cachedFlows = [];

  async function loadFlows() {
    const container = el('flows-grid-container');
    const countAll = el('flows-count-all');
    const countVideo = el('flows-count-video');
    const countOther = el('flows-count-other');

    try {
      const res = await apiFetch('/api/v1/flows');
      const data = await res.json();
      cachedFlows = Array.isArray(data) ? data : [];

      if (countAll) countAll.textContent = String(cachedFlows.length);
      const videoCount = cachedFlows.filter(f => {
        const name = (f.name || '').toLowerCase();
        return name.includes('mkv') || name.includes('vidéo') || name.includes('mp4') || name.includes('hevc');
      }).length;
      if (countVideo) countVideo.textContent = String(videoCount);
      if (countOther) countOther.textContent = String(cachedFlows.length - videoCount);

      renderFlowsGrid();
    } catch {
      if (container && cachedFlows.length === 0) {
        renderFlowsGrid();
      }
    }
  }

  function renderFlowsGrid() {
    const container = el('flows-grid-container');
    if (!container) return;

    if (cachedFlows.length === 0) {
      container.innerHTML = `
        <div class="empty-state-panel" id="flows-empty-state" style="grid-column: 1 / -1; text-align: center; padding: 2.5rem 1rem; border: 1px dashed var(--border-color); border-radius: var(--radius-md); background: var(--bg-card);">
          <div style="font-size: 2rem; margin-bottom: 0.5rem;" aria-hidden="true">⚡</div>
          <h3 style="margin: 0 0 0.5rem 0; font-size: 1.125rem; font-weight: 600;">Aucun flux de traitement installé</h3>
          <p style="margin: 0 auto 1.25rem auto; max-width: 500px; font-size: 0.875rem; color: var(--text-muted);">
            L'application démarre en moteur nu (Bare Core). Importez des flux certifiés depuis le catalogue de plugins ou concevez un pipeline sur-mesure.
          </p>
          <div style="display: flex; gap: 0.75rem; justify-content: center; flex-wrap: wrap;">
            <button type="button" id="btn-empty-import-flow" class="btn btn-secondary">
              🔌 Consulter le catalogue de plugins
            </button>
            <button type="button" id="btn-empty-create-flow" class="btn btn-primary">
              + Créer un flux (Assistant)
            </button>
          </div>
        </div>
      `;
      const btnImport = el('btn-empty-import-flow');
      if (btnImport) btnImport.addEventListener('click', () => switchTab('plugins'));
      const btnCreate = el('btn-empty-create-flow');
      if (btnCreate) {
        const wizard = el('flow-wizard-dialog');
        if (wizard) btnCreate.addEventListener('click', () => wizard.showModal());
      }
      return;
    }

    container.innerHTML = cachedFlows.map(flow => {
      const safeId = escapeHtml(flow.id);
      const safeName = escapeHtml(flow.name);
      const safeDesc = escapeHtml(flow.description || 'Flux de traitement modulaire');
      const isDefault = flow.is_default;
      const nameLower = flow.name.toLowerCase();
      const isVideo = nameLower.includes('mkv') || nameLower.includes('mp4') || nameLower.includes('vidéo') || nameLower.includes('hevc');
      const flowType = isVideo ? 'video' : 'other';

      let nodes = [];
      try {
        const def = typeof flow.definition === 'string' ? JSON.parse(flow.definition) : flow.definition;
        nodes = def.nodes || [];
      } catch {}

      const pipelineHtml = nodes.slice(0, 4).map((n, idx) => `
        <div class="flow-block-step">
          <span class="step-badge">${idx + 1}</span>
          <span class="step-name">${escapeHtml(n.name || n.type)}</span>
        </div>
      `).join('<span class="flow-arrow" aria-hidden="true">➔</span>');

      return `
        <article class="flow-card ${isDefault ? 'active-flow' : ''}" data-flow-id="${safeId}" data-flow-type="${flowType}">
          <div class="flow-card-header">
            <div>
              <div style="display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap;">
                <h3 class="flow-card-title">${safeName}</h3>
                ${isDefault ? '<span class="status-pill success"><span class="status-dot"></span>Défaut</span>' : ''}
                <span class="status-pill info">${flowType === 'video' ? '🎬 Vidéo' : '⚙️ Autre'}</span>
              </div>
              <p class="flow-card-desc">${safeDesc}</p>
            </div>
          </div>
          <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux">
            <div class="flow-pipeline">
              ${pipelineHtml || '<span style="font-size: 0.8125rem; color: var(--text-dim);">Pipeline vide</span>'}
            </div>
          </div>
          <div class="flow-card-footer">
            <span class="flow-pattern-tag">${escapeHtml(flow.file_mask || 'Filtre automatique')}</span>
            <div style="display: flex; gap: 0.5rem;">
              <button type="button" class="btn btn-primary btn-sm open-canvas-btn" data-flow="${safeId}">
                Ouvrir l'éditeur visuel (Canvas)
              </button>
            </div>
          </div>
        </article>
      `;
    }).join('');

    container.querySelectorAll('.open-canvas-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const flowId = btn.getAttribute('data-flow');
        const card = btn.closest('.flow-card');
        const flowTitle = card ? (card.querySelector('.flow-card-title')?.textContent || flowId) : flowId;
        openCanvas(flowId, flowTitle);
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

    // Filtrage des flux par type de média (Vidéo, Audio, Image, Système)
    document.querySelectorAll('.flow-type-chip').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.flow-type-chip').forEach(b => {
          b.classList.remove('active');
          b.setAttribute('aria-pressed', 'false');
        });
        btn.classList.add('active');
        btn.setAttribute('aria-pressed', 'true');
        const filter = btn.getAttribute('data-flow-filter') || 'all';

        document.querySelectorAll('.flow-card[data-flow-type]').forEach(card => {
          const cardType = card.getAttribute('data-flow-type');
          if (filter === 'all' || cardType === filter) {
            card.style.display = '';
          } else {
            card.style.display = 'none';
          }
        });
      });
    });

    // 1. Navigation entre Bibliothèque et Canvas
    function openCanvas(flowId, flowTitle) {
      activeCanvasFlowId = flowId;
      const tabCanvas = el('btn-subtab-flows-canvas');
      if (tabCanvas) {
        switchSubtab(tabCanvas);
      }
      if (libraryView) libraryView.style.display = 'none';
      if (canvasView) canvasView.style.display = 'block';
      const subtabCanvas = el('subtab-flows-canvas');
      if (subtabCanvas) subtabCanvas.removeAttribute('hidden');
      const subtabInstalled = el('subtab-flows-installed');
      if (subtabInstalled) subtabInstalled.setAttribute('hidden', '');
      const titleEl = el('canvas-flow-title');
      if (titleEl) titleEl.textContent = `Éditeur visuel : ${flowTitle}`;

      // Si le flux ouvert est le flux ADR-0008, injecter ses nœuds représentatifs
      if (flowId === 'flow.smart.adr0008' && nodesContainer) {
        nodesContainer.innerHTML = `
          <div class="canvas-node selected" id="node-1" data-node-id="node-1" data-block-type="file-input">
            <div class="node-connector connector-out"></div>
            <div class="node-header">
              <span class="node-icon">📄</span>
              <div class="node-title-box">
                <span class="node-kind">Déclencheur entrant</span>
                <h4 class="node-name">Fichier source</h4>
              </div>
              <span class="status-pill success node-pill">Entrée</span>
            </div>
            <div class="node-summary">Détection du média • Métadonnées analysées implicitement en tâche de fond (MediaInfo / FFprobe).</div>
          </div>

          <div class="canvas-node" id="node-2" data-node-id="node-2" data-block-type="condition">
            <div class="node-connector connector-in"></div>
            <div class="node-header">
              <span class="node-icon">🔀</span>
              <div class="node-title-box">
                <span class="node-kind">Logique décisionnelle</span>
                <h4 class="node-name">Condition : Codec == HEVC ?</h4>
              </div>
              <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
            </div>
            <div class="node-summary" id="node-2-summary">Si Codec == HEVC ➔ Édition In-Place directe • Sinon ➔ Transcodage GPU • Repli ➔ CPU</div>
            <div class="node-multi-outputs">
              <div class="multi-output-port port-true" title="Condition vérifiée : saut direct vers en-têtes sans réencodage">
                <span class="port-label">✓ Vrai (Match)</span>
                <div class="node-connector connector-out out-true" data-port="true"></div>
              </div>
              <div class="multi-output-port port-false" title="Condition non vérifiée : réencodage vidéo nécessaire">
                <span class="port-label">✗ Faux (Else)</span>
                <div class="node-connector connector-out out-false" data-port="false"></div>
              </div>
              <div class="multi-output-port port-fallback" title="Repli logiciel automatique en cas d'incompatibilité GPU">
                <span class="port-label">⚡ Repli (CPU)</span>
                <div class="node-connector connector-out out-fallback" data-port="fallback"></div>
              </div>
            </div>
          </div>

          <div class="canvas-node" id="node-3" data-node-id="node-3" data-block-type="mkvpropedit">
            <div class="node-connector connector-in"></div>
            <div class="node-connector connector-out"></div>
            <div class="node-header">
              <span class="node-icon">⚡</span>
              <div class="node-title-box">
                <span class="node-kind">MKVToolNix • In-Place</span>
                <h4 class="node-name">Édition En-têtes (mkvpropedit)</h4>
              </div>
              <span class="status-pill success node-pill in-place">⚡ In-Place</span>
              <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
            </div>
            <div class="node-summary" id="node-3-summary">Zéro copie • Piste audio 'fre' par défaut • Flag 'forced' sur sous-titres complets.</div>
          </div>

          <div class="canvas-node" id="node-4" data-node-id="node-4" data-block-type="video-encode">
            <div class="node-connector connector-in"></div>
            <div class="node-connector connector-out"></div>
            <div class="node-header">
              <span class="node-icon">🎞️</span>
              <div class="node-title-box">
                <span class="node-kind">FFmpeg Builder</span>
                <h4 class="node-name">Transcodage Vidéo HEVC</h4>
              </div>
              <span class="status-pill info node-pill working-file">📦 WorkingFile</span>
              <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
            </div>
            <div class="node-summary" id="node-4-summary">HEVC • Accélération GPU Intel QSV (Qualité : 6) • Repli CPU libx265 automatique.</div>
          </div>

          <div class="canvas-node" id="node-5" data-node-id="node-5" data-block-type="file-output">
            <div class="node-connector connector-in"></div>
            <div class="node-header">
              <span class="node-icon">💾</span>
              <div class="node-title-box">
                <span class="node-kind">Système Unix</span>
                <h4 class="node-name">Finalisation & Notification</h4>
              </div>
              <span class="status-pill success node-pill">Sortie</span>
            </div>
            <div class="node-summary" id="node-5-summary">Remplacement atomique POSIX Unix • Préservation mtime • Notification Sonarr/Radarr.</div>
          </div>
        `;
        rebindAllNodes();
      }

      resetSimulation();
      setTimeout(drawCanvasLines, 60);
    }

    function closeCanvas() {
      const tabInstalled = el('btn-subtab-flows-installed');
      if (tabInstalled) {
        switchSubtab(tabInstalled);
      }
      if (canvasView) canvasView.style.display = 'none';
      if (libraryView) libraryView.style.display = 'block';
      const subtabCanvas = el('subtab-flows-canvas');
      if (subtabCanvas) subtabCanvas.setAttribute('hidden', '');
      const subtabInstalled = el('subtab-flows-installed');
      if (subtabInstalled) subtabInstalled.removeAttribute('hidden');
    }

    if (backToLibraryBtn) backToLibraryBtn.addEventListener('click', closeCanvas);

    document.querySelectorAll('.open-canvas-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const card = btn.closest('.flow-card');
        const title = card ? card.querySelector('.flow-card-title').textContent : 'Flux';
        const flowId = btn.getAttribute('data-flow') || 'flow';
        openCanvas(flowId, title);
      });
    });

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

    if (generateWizardBtn) {
      generateWizardBtn.addEventListener('click', () => {
        const nameInput = el('wizard-flow-name');
        const flowTitle = nameInput && nameInput.value ? nameInput.value.trim() : 'Nouveau flux';
        closeWizard();

        // Reconstituer les nœuds selon le template choisi
        if (nodesContainer) {
          nodesContainer.innerHTML = `
            <div class="canvas-node selected" id="node-1" data-node-id="node-1" data-block-type="file-input">
              <div class="node-connector connector-out"></div>
              <div class="node-header">
                <span class="node-icon">📄</span>
                <div class="node-title-box">
                  <span class="node-kind">Déclencheur entrant</span>
                  <h4 class="node-name">Fichier source</h4>
                </div>
                <span class="status-pill success node-pill">Entrée</span>
              </div>
              <div class="node-summary">Détection du média • Métadonnées inspectées automatiquement en sous-main.</div>
            </div>
          `;

          if (currentSelectedTemplate === 'smart-hevc') {
            appendNodeToCanvas('condition', 'Logique de décision', 'Condition : Est déjà HEVC ?', 'Si Codec == HEVC ➔ In-Place mkvpropedit • Sinon ➔ Encodage QSV • Repli ➔ CPU');
            appendNodeToCanvas('mkvpropedit', 'MKVToolNix', 'Édition En-têtes (In-Place)', 'Zéro copie dans /work • Mise à jour des drapeaux audio/sous-titres');
            appendNodeToCanvas('video-encode', 'FFmpeg Builder', 'Réencodage Vidéo (GPU)', 'HEVC • Intel QSV • Qualité : 6 • Vitesse : 3');
            appendNodeToCanvas('file-output', 'Système', 'Destination finale', 'Remplacement sécurisé de l\'original ou dossier de sortie');
          } else if (currentSelectedTemplate === 'hevc') {
            appendNodeToCanvas('video-encode', 'FFmpeg Builder', 'Réencodage Vidéo', 'HEVC • Intel QSV • Qualité : 6 • Vitesse : 3');
            appendNodeToCanvas('audio-encode', 'FFmpeg Builder', 'Réencodage Audio', 'Codec : AAC • 192 kbps • Stéréo');
            appendNodeToCanvas('remux-mkv', 'MKVToolNix', 'Remuxage Matroska', 'Assemblage final MKV • Langues : fre, eng');
          } else if (currentSelectedTemplate === 'mkv-remux') {
            appendNodeToCanvas('remux-mkv', 'MKVToolNix', 'Remuxage Matroska', 'Filtrage langues fre, eng • Copie directe sans transcodage');
          } else if (currentSelectedTemplate === 'mp4-web') {
            appendNodeToCanvas('remux-mp4', 'MP4Box GPAC', 'Remuxage MP4', 'FastStart activé • Moov atom en début de fichier');
          }
        }

        openCanvas('generated', flowTitle);
        showNotice(`Flux « ${flowTitle} » généré avec succès ! Prêt dans le Canvas.`, 'success');
      });
    }

    // 3. Tracé des connexions SVG entre les connecteurs
    function drawCanvasLines() {
      if (!svgLines || !nodesContainer) return;
      const nodes = Array.from(nodesContainer.querySelectorAll('.canvas-node'));
      if (nodes.length < 2) {
        svgLines.innerHTML = '';
        return;
      }

      const boardRect = nodesContainer.parentElement.getBoundingClientRect();
      let pathsHtml = `
        <defs>
          <marker id="arrow-blue" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--primary)" />
          </marker>
          <marker id="arrow-green" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="#10b981" />
          </marker>
          <marker id="arrow-orange" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="#f59e0b" />
          </marker>
          <marker id="arrow-purple" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="#8b5cf6" />
          </marker>
        </defs>
      `;

      for (let i = 0; i < nodes.length - 1; i++) {
        const fromNode = nodes[i];
        const toNode = nodes[i + 1];

        const outConnectors = Array.from(fromNode.querySelectorAll('.connector-out'));
        const inConn = toNode.querySelector('.connector-in');

        if (outConnectors.length > 0 && inConn) {
          const rIn = inConn.getBoundingClientRect();
          const x2 = rIn.left + rIn.width / 2 - boardRect.left + nodesContainer.parentElement.scrollLeft;
          const y2 = rIn.top + rIn.height / 2 - boardRect.top + nodesContainer.parentElement.scrollTop;

          outConnectors.forEach(outConn => {
            const rOut = outConn.getBoundingClientRect();
            const x1 = rOut.left + rOut.width / 2 - boardRect.left + nodesContainer.parentElement.scrollLeft;
            const y1 = rOut.top + rOut.height / 2 - boardRect.top + nodesContainer.parentElement.scrollTop;

            let strokeColor = 'var(--primary)';
            let markerId = 'arrow-blue';
            let isPulse = false;

            if (outConn.classList.contains('out-true')) {
              strokeColor = '#10b981';
              markerId = 'arrow-green';
              if (activeSimulation === 'true') isPulse = true;
            } else if (outConn.classList.contains('out-false')) {
              strokeColor = '#f59e0b';
              markerId = 'arrow-orange';
              if (activeSimulation === 'false') isPulse = true;
            } else if (outConn.classList.contains('out-fallback')) {
              strokeColor = '#8b5cf6';
              markerId = 'arrow-purple';
              if (activeSimulation === 'fallback') isPulse = true;
            }

            const cy1 = y1 + 18;
            const cy2 = y2 - 18;
            const pulseClass = isPulse ? ' class="sim-pulse-line"' : '';
            pathsHtml += `<path d="M ${x1} ${y1} C ${x1} ${cy1}, ${x2} ${cy2}, ${x2} ${y2}" stroke="${strokeColor}" stroke-width="${isPulse ? '3.5' : '2.5'}" fill="none" marker-end="url(#${markerId})"${pulseClass} />`;
          });
        }
      }
      svgLines.innerHTML = pathsHtml;
    }

    window.addEventListener('resize', drawCanvasLines);

    // 4. Ajout d'un bloc au canvas
    function appendNodeToCanvas(blockType, kind, title, summary) {
      if (!nodesContainer) return;
      const count = nodesContainer.querySelectorAll('.canvas-node').length + 1;
      const nodeId = `node-${count}`;
      const icon = blockType === 'condition' ? '🔀' : blockType === 'mkvpropedit' ? '⚡' : blockType === 'file-output' ? '💾' : blockType === 'sonarr' ? '📡' : blockType === 'radarr' ? '🎬' : blockType.includes('video') ? '🎞️' : blockType.includes('audio') ? '🔊' : blockType.includes('mp4') ? '📦' : '🎬';

      const div = document.createElement('div');
      div.className = 'canvas-node';
      div.id = nodeId;
      div.setAttribute('data-node-id', nodeId);
      div.setAttribute('data-block-type', blockType);

      if (blockType === 'condition') {
        div.innerHTML = `
          <div class="node-connector connector-in"></div>
          <div class="node-header">
            <span class="node-icon">🔀</span>
            <div class="node-title-box">
              <span class="node-kind">${kind}</span>
              <h4 class="node-name">${title}</h4>
            </div>
            <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
          </div>
          <div class="node-summary" id="${nodeId}-summary">${summary}</div>
          <div class="node-multi-outputs">
            <div class="multi-output-port port-true" title="Condition vérifiée : saut direct vers en-têtes In-Place">
              <span class="port-label">✓ Vrai (Match)</span>
              <div class="node-connector connector-out out-true" data-port="true"></div>
            </div>
            <div class="multi-output-port port-false" title="Condition non vérifiée : réencodage nécessaire">
              <span class="port-label">✗ Faux (Else)</span>
              <div class="node-connector connector-out out-false" data-port="false"></div>
            </div>
            <div class="multi-output-port port-fallback" title="Repli d'urgence (si échec GPU)">
              <span class="port-label">⚡ Repli (CPU)</span>
              <div class="node-connector connector-out out-fallback" data-port="fallback"></div>
            </div>
          </div>
        `;
      } else if (blockType === 'mkvpropedit') {
        div.innerHTML = `
          <div class="node-connector connector-in"></div>
          <div class="node-connector connector-out"></div>
          <div class="node-header">
            <span class="node-icon">⚡</span>
            <div class="node-title-box">
              <span class="node-kind">${kind}</span>
              <h4 class="node-name">${title}</h4>
            </div>
            <span class="status-pill success node-pill in-place">⚡ In-Place</span>
            <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
          </div>
          <div class="node-summary" id="${nodeId}-summary">${summary}</div>
        `;
      } else {
        const isWorkingFile = blockType.includes('video') || blockType.includes('audio') || blockType.includes('remux');
        const pillHtml = isWorkingFile ? '<span class="status-pill info node-pill working-file">📦 WorkingFile</span>' : '';
        div.innerHTML = `
          <div class="node-connector connector-in"></div>
          ${blockType === 'file-output' ? '' : '<div class="node-connector connector-out"></div>'}
          <div class="node-header">
            <span class="node-icon">${icon}</span>
            <div class="node-title-box">
              <span class="node-kind">${kind}</span>
              <h4 class="node-name">${title}</h4>
            </div>
            ${pillHtml}
            <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
          </div>
          <div class="node-summary" id="${nodeId}-summary">${summary}</div>
        `;
      }

      nodesContainer.appendChild(div);
      bindNodeEvents(div);
      selectNode(div);
      setTimeout(drawCanvasLines, 30);
    }

    // 5. Sélection et inspecteur de bloc
    function selectNode(nodeEl) {
      if (!nodeEl) return;
      document.querySelectorAll('.canvas-node').forEach(n => n.classList.remove('selected'));
      nodeEl.classList.add('selected');

      const blockType = nodeEl.getAttribute('data-block-type') || 'file-input';
      const nodeName = nodeEl.querySelector('.node-name') ? nodeEl.querySelector('.node-name').textContent : 'Bloc';
      const nodeKind = nodeEl.querySelector('.node-kind') ? nodeEl.querySelector('.node-kind').textContent : 'Outil';

      const titleEl = el('inspector-block-title');
      const toolEl = el('inspector-block-tool');
      const formContainer = el('inspector-form-container');

      if (titleEl) titleEl.textContent = nodeName;
      if (toolEl) toolEl.textContent = nodeKind.split('•')[0].trim();

      if (formContainer) {
        if (blockType === 'file-input') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label for="node-cfg-mask" class="form-label">Masque de surveillance</label>
              <input type="text" id="node-cfg-mask" class="form-input" value="*.mkv, *.mp4, *.avi, *.ts" />
              <span class="form-help">Filtre les extensions déclenchant ce flux.</span>
            </div>
            <div class="status-pill success" style="width: 100%; margin-top: 0.75rem; justify-content: center;">
              ✓ Métadonnées inspectées automatiquement
            </div>
            <p style="font-size: 0.8125rem; color: var(--text-dim); line-height: 1.5; margin-top: 0.5rem;">
              Conformément à l'ADR 0008, les métadonnées (codecs, résolutions, langues audio et sous-titres) sont injectées automatiquement par MediaInfo et FFprobe dès l'interception du média.
            </p>
          `;
        } else if (blockType === 'condition') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label for="node-cfg-prop" class="form-label">Propriété à tester</label>
              <select id="node-cfg-prop" class="form-select">
                <option value="video.codec" selected>Codec Vidéo</option>
                <option value="video.resolution">Résolution (1080p, 4K...)</option>
                <option value="audio.language">Langue Audio présente</option>
                <option value="video.hdr">Profil HDR (HDR10, DV)</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-op" class="form-label">Opérateur</label>
              <select id="node-cfg-op" class="form-select">
                <option value="eq" selected>Est égal à (==)</option>
                <option value="neq">Est différent de (!=)</option>
                <option value="contains">Contient</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-val" class="form-label">Valeur cible</label>
              <input type="text" id="node-cfg-val" class="form-input" value="HEVC" />
            </div>
            <div style="background: var(--bg-subtle); padding: 0.75rem; border-radius: var(--radius-sm); border: 1px solid var(--border-color); font-size: 0.75rem; line-height: 1.45;">
              <strong style="color: var(--text-main);">Aiguillage tri-sorties (ADR-0008) :</strong>
              <ul style="margin: 0.25rem 0 0 1rem; color: var(--text-dim);">
                <li><span style="color: #10b981; font-weight: 700;">✓ Vrai :</span> Saut immédiat vers In-Place mkvpropedit</li>
                <li><span style="color: #f59e0b; font-weight: 700;">✗ Faux :</span> Encodage matériel GPU dans /work</li>
                <li><span style="color: #8b5cf6; font-weight: 700;">⚡ Repli :</span> Bascule automatique CPU libx265</li>
              </ul>
            </div>
          `;
        } else if (blockType === 'mkvpropedit') {
          formContainer.innerHTML = `
            <div class="status-pill success in-place" style="width: 100%; margin-bottom: 0.75rem; justify-content: center;">
              ⚡ Exécution In-Place (ADR 0008)
            </div>
            <p style="font-size: 0.8125rem; color: var(--text-dim); margin-bottom: 0.75rem;">
              Modifie directement les en-têtes EBML Matroska sur le fichier sans aucune duplication préalable dans /work (gain massif en E/S disque).
            </p>
            <div class="form-group">
              <label for="prop-audio-default" class="form-label">Piste audio par défaut</label>
              <input type="text" id="prop-audio-default" class="form-input" value="fre" />
              <span class="form-help">Code langue ISO-639-2 (ex: fre, fra)</span>
            </div>
            <div class="form-group">
              <label class="form-label">Drapeaux sous-titres</label>
              <label style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.8125rem;">
                <input type="checkbox" checked /> Forcer drapeau 'forced' si piste forcée détectée
              </label>
            </div>
          `;
        } else if (blockType === 'video-encode') {
          formContainer.innerHTML = `
            <div class="status-pill info working-file" style="width: 100%; margin-bottom: 0.75rem; justify-content: center;">
              📦 WorkingFile éphémère (/work)
            </div>
            <div class="form-group">
              <label for="node-cfg-codec" class="form-label">Codec cible</label>
              <select id="node-cfg-codec" class="form-select">
                <option value="hevc" selected>HEVC / H.265 (Recommandé)</option>
                <option value="av1">AV1 (Haute efficacité)</option>
                <option value="h264">H.264 / AVC (Universel)</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-encoder" class="form-label">Encodeur matériel</label>
              <select id="node-cfg-encoder" class="form-select">
                <option value="qsv" selected>Intel QuickSync (hevc_qsv)</option>
                <option value="nvenc">NVIDIA NVENC (hevc_nvenc)</option>
                <option value="vaapi">Linux VA-API (hevc_vaapi)</option>
                <option value="cpu">Processeur CPU (libx265)</option>
              </select>
            </div>
            <div class="form-group">
              <label class="form-label">Qualité cible (1 = Débit bas, 9 = Transparent)</label>
              <input type="range" min="1" max="9" value="6" class="form-range" />
            </div>
          `;
        } else if (blockType === 'file-output') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label for="node-cfg-dest-mode" class="form-label">Mode de destination</label>
              <select id="node-cfg-dest-mode" class="form-select">
                <option value="replace" selected>Remplacer l'original (Atomique POSIX)</option>
                <option value="folder">Dossier de sortie dédié (/data/output)</option>
              </select>
              <span class="form-help">Remplacement atomique Unix garantissant zéro fichier corrompu.</span>
            </div>
            <div class="form-group">
              <label class="form-label">Intégration média</label>
              <label style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.8125rem;">
                <input type="checkbox" checked /> Notifier Sonarr / Radarr du renommage
              </label>
            </div>
          `;
        } else {
          formContainer.innerHTML = `
            <div class="form-group">
              <label class="form-label">Paramètres généraux du bloc</label>
              <input type="text" class="form-input" value="Configuration par défaut" />
            </div>
          `;
        }
      }
    }

    function bindNodeEvents(nodeEl) {
      if (!nodeEl) return;
      nodeEl.addEventListener('click', (e) => {
        if (!e.target.classList.contains('node-remove-btn')) {
          selectNode(nodeEl);
        }
      });

      const removeBtn = nodeEl.querySelector('.node-remove-btn');
      if (removeBtn) {
        removeBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          nodeEl.remove();
          drawCanvasLines();
          const firstRemaining = nodesContainer.querySelector('.canvas-node');
          if (firstRemaining) selectNode(firstRemaining);
        });
      }
    }

    function rebindAllNodes() {
      if (!nodesContainer) return;
      nodesContainer.querySelectorAll('.canvas-node').forEach(nodeEl => {
        bindNodeEvents(nodeEl);
      });
      const first = nodesContainer.querySelector('.canvas-node');
      if (first) selectNode(first);
    }

    rebindAllNodes();

    // 6. Ajout de blocs depuis la barre latérale
    document.querySelectorAll('.draggable-block-item.addable').forEach(item => {
      item.addEventListener('click', () => {
        const blockType = item.getAttribute('data-block-type') || 'remux-mkv';
        const title = item.querySelector('strong') ? item.querySelector('strong').textContent : 'Nouveau bloc';
        const kind = item.querySelector('span') ? item.querySelector('span').textContent : 'Outil';
        const desc = item.querySelector('.block-drag-info span') ? item.querySelector('.block-drag-info span').textContent : '';

        appendNodeToCanvas(blockType, kind, title, desc);
        showNotice(`Bloc « ${title} » ajouté au flux`, 'info');
      });
    });

    // 7. Banc d'essai de simulation interactive ADR 0008
    function resetSimulation() {
      activeSimulation = null;
      document.querySelectorAll('.sim-btn').forEach(b => {
        b.classList.remove('active-true', 'active-false', 'active-fallback');
      });
      if (nodesContainer) {
        nodesContainer.querySelectorAll('.canvas-node').forEach(n => {
          n.classList.remove('sim-active', 'sim-active-true', 'sim-active-false', 'sim-active-fallback', 'sim-dimmed');
        });
      }
      drawCanvasLines();
    }

    document.querySelectorAll('.sim-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const simType = btn.getAttribute('data-sim');
        if (simType === 'reset') {
          resetSimulation();
          showNotice('Simulation réinitialisée', 'info');
          return;
        }

        resetSimulation();
        activeSimulation = simType;

        if (simType === 'true') {
          btn.classList.add('active-true');
          const n1 = el('node-1');
          const n2 = el('node-2');
          const n3 = el('node-3');
          const n4 = el('node-4');
          const n5 = el('node-5');

          if (n1) n1.classList.add('sim-active');
          if (n2) n2.classList.add('sim-active', 'sim-active-true');
          if (n3) n3.classList.add('sim-active', 'sim-active-true');
          if (n4) n4.classList.add('sim-dimmed');
          if (n5) n5.classList.add('sim-active');

          drawCanvasLines();
          showNotice('✓ Simulation ADR-0008 : Fichier HEVC détecté. Traitement In-Place direct via mkvpropedit (aucun transfert /work).', 'success');
        } else if (simType === 'false') {
          btn.classList.add('active-false');
          const n1 = el('node-1');
          const n2 = el('node-2');
          const n3 = el('node-3');
          const n4 = el('node-4');
          const n5 = el('node-5');

          if (n1) n1.classList.add('sim-active');
          if (n2) n2.classList.add('sim-active', 'sim-active-false');
          if (n3) n3.classList.add('sim-dimmed');
          if (n4) n4.classList.add('sim-active', 'sim-active-false');
          if (n5) n5.classList.add('sim-active');

          drawCanvasLines();
          showNotice('✗ Simulation ADR-0008 : Fichier non HEVC. Routage vers l\'encodeur matériel GPU dans /work/job-<uuid>/.', 'info');
        } else if (simType === 'fallback') {
          btn.classList.add('active-fallback');
          const n1 = el('node-1');
          const n2 = el('node-2');
          const n3 = el('node-3');
          const n4 = el('node-4');
          const n5 = el('node-5');

          if (n1) n1.classList.add('sim-active');
          if (n2) n2.classList.add('sim-active', 'sim-active-fallback');
          if (n3) n3.classList.add('sim-dimmed');
          if (n4) {
            n4.classList.add('sim-active', 'sim-active-fallback');
            const summary = el('node-4-summary');
            if (summary) summary.textContent = '⚡ Bascule repli logiciel active : CPU libx265 (Qualité : 6)';
          }
          if (n5) n5.classList.add('sim-active');

          drawCanvasLines();
          showNotice('⚡ Simulation ADR-0008 : Échec transcodeur matériel GPU. Bascule automatique vers l\'encodeur CPU logiciel.', 'warning');
        }
      });
    });

    // Sauvegarde des modifications du flux en base SQLite
    const saveFlowBtn = el('btn-canvas-save-flow');
    if (saveFlowBtn) {
      saveFlowBtn.addEventListener('click', async () => {
        const titleEl = el('canvas-flow-title');
        const title = titleEl ? titleEl.textContent.replace('Éditeur visuel :', '').trim() : 'Flux';
        const flowId = activeCanvasFlowId || 'flow.custom';

        const nodes = Array.from(nodesContainer.querySelectorAll('.canvas-node')).map(n => ({
          id: n.getAttribute('data-node-id') || n.id,
          type: n.getAttribute('data-block-type') || 'unknown',
          name: n.querySelector('.node-name') ? n.querySelector('.node-name').textContent : 'Bloc',
        }));

        const definition = JSON.stringify({ nodes, connections: [] });

        try {
          const res = await apiFetch(`/api/v1/flows/${encodeURIComponent(flowId)}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              name: title,
              description: 'Flux configuré depuis le plan de travail visuel.',
              file_mask: '*.mkv, *.mp4',
              is_default: flowId === 'flow.default.mkv',
              definition,
            }),
          });
          if (res.ok) {
            showNotice(`Flux « ${title} » enregistré avec succès en base SQLite.`, 'success');
          } else {
            showNotice('Erreur lors de l\'enregistrement du flux.', 'error');
          }
        } catch (_) {
          showNotice(`Flux « ${title} » mis à jour en mémoire active.`, 'info');
        }
      });
    }
  }
