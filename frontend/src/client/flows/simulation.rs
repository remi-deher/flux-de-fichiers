pub const SCRIPT: &str = r###"
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

    // Enregistrement : le graphe édité complet, avec les métadonnées du flux ouvert.
    const saveFlowBtn = el('btn-canvas-save-flow');
    if (saveFlowBtn) {
      saveFlowBtn.addEventListener('click', async () => {
        if (!editorFlow) {
          showNotice('Ouvrez ou créez un flux avant de l\'enregistrer.', 'info');
          return;
        }
        if (invalidConfigs.size > 0) {
          showNotice('Corrigez les réglages JSON invalides avant d\'enregistrer.', 'warning');
          return;
        }
        const flowId = editorFlow.id || newFlowId(editorFlow.name, Date.now().toString(36));
        try {
          const res = await apiFetch(`/api/v1/flows/${encodeURIComponent(flowId)}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(flowSaveRequest(editorFlow, editorGraph)),
          });
          const saved = await res.json();
          editorFlow = saved;
          showNotice(`Flux « ${saved.name} » enregistré (version ${saved.version}).`, 'success');
          loadFlows();
        } catch (_) {
          showNotice('Échec de l\'enregistrement : le flux n\'a pas été modifié.', 'error');
        }
      });
    }
  }

"###;
