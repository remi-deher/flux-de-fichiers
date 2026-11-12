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
    } catch (e) {
      if (tbody) {
        tbody.innerHTML = `<tr><td colspan="4" class="empty-state-text" style="color: var(--danger-text);">
          ${escapeHtml(e.message === 'Non autorisé' ? 'Veuillez ouvrir une session pour afficher les agents enregistrés.' : 'Impossible de récupérer la liste des agents.')}
        </td></tr>`;
      }
    }
  }

  function renderWorkersTable() {
    const tbody = el('workers-table-body');
    if (!tbody) return;

    if (cachedWorkers.length === 0) {
      tbody.innerHTML = `<tr><td colspan="4" class="empty-state-text">
        Aucun agent distant enregistré. Le coordinateur central traite les tâches en mode autonome (agent intégré).
      </td></tr>`;
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
      const deleteAction = !isOnline
        ? `<button type="button" class="btn btn-secondary btn-sm delete-worker-btn" data-worker-id="${safeId}" style="color: var(--danger-text); padding: 0.2rem 0.5rem; font-size: 0.75rem; margin-left: 0.5rem;" title="Supprimer cet agent hors ligne">Supprimer</button>`
        : '';
      return `
        <tr>
          <td style="font-family: var(--font-mono); font-weight: 600;">${safeId}</td>
          <td>
            <div style="font-weight: 600;">${safeName}</div>
            <div style="font-size: 0.75rem; color: var(--text-dim);">${isLocal ? 'Agent local intégré' : 'Agent distant HTTP'}</div>
          </td>
          <td>${formatDate(w.last_seen)}</td>
          <td>
            <div style="display: flex; align-items: center; justify-content: space-between;">
              ${statusHtml}
              ${deleteAction}
            </div>
          </td>
        </tr>
      `;
    }).join('');

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
    const configWorkerKey = el('worker-key-display');
    const btnToggleWorkerKey = el('btn-toggle-worker-key');
    const btnCopyWorkerKey = el('btn-copy-worker-key');
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

      if (configWorkerKey) {
        configWorkerKey.value = cfg.worker_key || '';
        configWorkerKey.placeholder = 'Clé prête';
      }
      if (btnToggleWorkerKey) btnToggleWorkerKey.disabled = false;
      if (btnCopyWorkerKey) btnCopyWorkerKey.disabled = false;

      const origin = window.location.origin || 'http://localhost:3000';
      if (dockerCmd) {
        const keyVal = cfg.worker_key || '<WORKER_KEY>';
        dockerCmd.textContent = `docker run -d --name flux-worker -e COORDINATOR_URL=${origin} -e WORKER_KEY=${keyVal} -v /chemin/data:/data flux-de-fichiers worker`;
      }
    } catch {
      cachedSystemConfig = null;
      if (configWorkerKey) {
        configWorkerKey.value = '';
        configWorkerKey.type = 'password';
        configWorkerKey.placeholder = 'Ouvrez une session admin pour révéler la clé';
      }
      if (btnToggleWorkerKey) {
        btnToggleWorkerKey.disabled = true;
        btnToggleWorkerKey.textContent = 'Afficher';
      }
      if (btnCopyWorkerKey) btnCopyWorkerKey.disabled = true;
      if (configLocalWorker) configLocalWorker.textContent = '— (session requise)';
      if (configVersion) configVersion.textContent = '—';
      if (configMaxBytes) configMaxBytes.textContent = '—';
      if (settingsLocalWorker) settingsLocalWorker.textContent = '— (session requise)';
      if (settingsVersion) settingsVersion.textContent = '—';

      const origin = window.location.origin || 'http://localhost:3000';
      if (dockerCmd) {
        dockerCmd.textContent = `docker run -d --name flux-worker -e COORDINATOR_URL=${origin} -e WORKER_KEY=<WORKER_KEY> -v /chemin/data:/data flux-de-fichiers worker`;
      }
    }
  }

  function setupWorkerConfigUi() {
    const btnToggle = el('btn-toggle-worker-key');
    const inputKey = el('worker-key-display');
    const btnCopyKey = el('btn-copy-worker-key');
    const btnCopyCmd = el('btn-copy-worker-cmd');
    const dockerCmd = el('docker-worker-cmd');

    if (btnToggle && inputKey) {
      btnToggle.addEventListener('click', () => {
        if (inputKey.type === 'password') {
          inputKey.type = 'text';
          btnToggle.textContent = 'Masquer';
        } else {
          inputKey.type = 'password';
          btnToggle.textContent = 'Afficher';
        }
      });
    }

    if (btnCopyKey && inputKey) {
      btnCopyKey.addEventListener('click', async () => {
        if (!inputKey.value) return;
        try {
          await navigator.clipboard.writeText(inputKey.value);
          showNotice('Clé worker copiée dans le presse-papier !', 'success');
        } catch {
          inputKey.select();
          document.execCommand('copy');
          showNotice('Clé worker copiée !', 'success');
        }
      });
    }

    if (btnCopyCmd && dockerCmd) {
      btnCopyCmd.addEventListener('click', async () => {
        const text = dockerCmd.textContent;
        if (!text) return;
        try {
          await navigator.clipboard.writeText(text);
          showNotice('Commande Docker copiée dans le presse-papier !', 'success');
        } catch {
          showNotice('Veuillez copier manuellement la commande.', 'info');
        }
      });
    }

    const btnRefresh = el('btn-refresh-workers');
    if (btnRefresh) {
      btnRefresh.addEventListener('click', async () => {
        btnRefresh.disabled = true;
        try {
          await Promise.all([loadWorkers(), loadSystemConfig()]);
          showNotice('État des serveurs et configuration actualisés.', 'info');
        } catch {
          showNotice('Erreur lors du rafraîchissement des serveurs.', 'error');
        } finally {
          btnRefresh.disabled = false;
        }
      });
    }
  }

"###;
