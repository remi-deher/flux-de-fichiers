//! Gestion des agents et configuration.

pub const SCRIPT: &str = r###"
// --- Serveurs et agents ---
  async function loadWorkers() {
    const tbody = el('workers-table-body');
    const metricWorkers = el('metric-workers-count');

    try {
      const res = await apiFetch('/api/v1/workers');
      const data = await res.json();
      cachedWorkers = Array.isArray(data) ? data : (data.workers || []);

      const activeCount = cachedWorkers.filter(w => (Date.now() - new Date(w.last_seen).getTime()) < 120000).length;
      if (metricWorkers) metricWorkers.textContent = String(activeCount);
      updateHeaderWorkersCount(true, activeCount);
      renderWorkersTable();
      if (typeof renderLibrariesTable === 'function') renderLibrariesTable();
      if (typeof renderLibraryNodes === 'function') renderLibraryNodes();
      if (typeof renderLibraryWorkerSelect === 'function') renderLibraryWorkerSelect();
      markWidgetSource('workers', true);
    } catch (e) {
      markWidgetSource('workers', false);
      if (tbody) {
        tbody.innerHTML = emptyStateRow(5, escapeHtml(e.message === 'Non autorisé' ? 'Veuillez ouvrir une session pour afficher les agents enregistrés.' : 'Impossible de récupérer la liste des agents.'), true);
      }
    }
  }

  function renderWorkersTable() {
    const tbody = el('workers-table-body');
    if (!tbody) return;

    if (cachedWorkers.length === 0) {
      tbody.innerHTML = `<tr class="registre-row" data-worker-id="local">
          <td class="text-strong">Coordinateur local</td>
          <td>Coordinateur</td>
          <td>—</td>
          <td>${renderStatusPill('Intégré', 'info')}</td>
          <td class="cell-end"><button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-worker-id="local">Ouvrir</button></td>
      </tr>`;
      bindWorkerRows(tbody);
      return;
    }

    const now = Date.now();
    tbody.innerHTML = cachedWorkers.map(w => {
      const safeId = escapeHtml(w.id);
      const safeName = escapeHtml(w.name);
      const lastSeenMs = new Date(w.last_seen).getTime();
      const isOnline = (now - lastSeenMs) < 120000;
      const statusHtml = renderStatusPill(isOnline ? 'En ligne' : 'Hors ligne', isOnline ? 'online' : 'offline', { pulse: isOnline });
      const isLocal = safeName.startsWith('coordinator');
      const role = isLocal ? 'Coordinateur' : 'Agent';
      const deleteAction = !isOnline
        ? `<button type="button" class="btn btn-secondary btn-sm delete-worker-btn btn-danger-quiet" data-worker-id="${safeId}" title="Supprimer cet agent hors ligne">Supprimer</button>`
        : '';
      return `<tr class="registre-row" data-worker-id="${safeId}">
          <td class="text-strong">${safeName}</td>
          <td>${role}</td>
          <td>${formatDate(w.last_seen)}</td>
          <td>${statusHtml}</td>
          <td class="cell-end">
            <div class="inline-actions">
              <button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-worker-id="${safeId}">Ouvrir</button>
              ${deleteAction}
            </div>
          </td>
      </tr>`;
    }).join('');
    bindWorkerRows(tbody);
  }

  function bindWorkerRows(tbody) {
    tbody.querySelectorAll('.registre-open-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const id = btn.getAttribute('data-worker-id');
        const worker = cachedWorkers.find(item => item.id === id);
        const row = btn.closest('tr');
        markRegistreRow(tbody, row);
        if (!worker) {
          showRegistreFact('workers-fact', 'Coordinateur local', ['Coordinateur', 'Agent intégré']);
          return;
        }
        const isOnline = (Date.now() - new Date(worker.last_seen).getTime()) < 120000;
        const role = String(worker.name || '').startsWith('coordinator') ? 'Coordinateur' : 'Agent';
        showRegistreFact('workers-fact', worker.name || worker.id, [role, isOnline ? 'En ligne' : 'Hors ligne', formatDate(worker.last_seen)]);
      });
    });

    tbody.querySelectorAll('.delete-worker-btn').forEach(btn => {
      btn.addEventListener('click', async (e) => {
        e.stopPropagation();
        const id = btn.getAttribute('data-worker-id');
        if (!id || !confirm('Confirmer la suppression de cet agent hors ligne ?')) return;
        try {
          const res = await apiFetch(`/api/v1/workers/${id}`, { method: 'DELETE' });
          if (!res.ok) throw new Error('Échec suppression');
          showNotice('Agent supprimé avec succès.', 'success');
          await loadWorkers();
        } catch {
          showNotice('Impossible de supprimer cet agent.', 'error');
        }
      });
    });
  }

  // --- Configuration système & raccordement *arr ---
  let cachedSystemConfig = null;

  async function loadSystemConfig() {
    const configVersion = el('config-version');
    const configLocalWorker = el('config-local-worker');
    const configMaxBytes = el('config-max-file-bytes');
    const settingsVersion = el('settings-version');
    const settingsLocalWorker = el('settings-local-worker');
    const dockerCmd = el('docker-worker-cmd');

    try {
      const res = await apiFetch('/api/v1/config');
      if (!res.ok) {
        throw new Error('Non autorisé');
      }
      const cfg = await res.json();
      cachedSystemConfig = cfg;

      if (configVersion) configVersion.textContent = 'v' + cfg.version;
      if (settingsVersion) settingsVersion.textContent = 'v' + cfg.version;
      const workerStatusText = cfg.local_worker ? 'Actif (intégré au conteneur)' : 'Désactivé (agents distants)';
      if (configLocalWorker) configLocalWorker.textContent = workerStatusText;
      if (settingsLocalWorker) settingsLocalWorker.textContent = workerStatusText;
      if (configMaxBytes) configMaxBytes.textContent = formatBytes(cfg.max_file_bytes);

      const origin = window.location.origin || 'http://localhost:3000';
      if (dockerCmd) {
        dockerCmd.textContent = `docker run -d --name flux-worker -e COORDINATOR_URL=${origin} -e WORKER_KEY=<CLÉ_API_WORKER> -v /chemin/data:/data flux-de-fichiers worker`;
      }
    } catch {
      cachedSystemConfig = null;
      if (configLocalWorker) configLocalWorker.textContent = '— (session requise)';
      if (configVersion) configVersion.textContent = '—';
      if (configMaxBytes) configMaxBytes.textContent = '—';
      if (settingsLocalWorker) settingsLocalWorker.textContent = '— (session requise)';
      if (settingsVersion) settingsVersion.textContent = '—';

      const origin = window.location.origin || 'http://localhost:3000';
      if (dockerCmd) {
        dockerCmd.textContent = `docker run -d --name flux-worker -e COORDINATOR_URL=${origin} -e WORKER_KEY=<CLÉ_API_WORKER> -v /chemin/data:/data flux-de-fichiers worker`;
      }
    }
  }

  function setupWorkerConfigUi() {
    const btnCopyCmd = el('btn-copy-worker-cmd');
    const dockerCmd = el('docker-worker-cmd');

    // L'action de capsule ouvre le tiroir directement sur le volet de raccordement.
    const connectBtn = el('btn-workers-connect');
    if (connectBtn) {
      connectBtn.addEventListener('click', () => {
        const drawer = el('workers-fact');
        markRegistreRow(el('workers-table-body'), null);
        showRegistreFact('workers-fact', 'Raccorder un nœud', []);
        const connectTab = el('workers-fact-tab-connect');
        if (drawer && connectTab) selectInspectionTab(drawer, connectTab);
      });
    }

    if (btnCopyCmd && dockerCmd) {
      btnCopyCmd.addEventListener('click', () => {
        const text = dockerCmd.textContent;
        if (text) copyWithFeedback(btnCopyCmd, text, 'Commande Docker copiée dans le presse-papier.');
      });
    }
  }

"###;
