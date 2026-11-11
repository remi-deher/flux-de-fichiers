// --- Serveurs et agents ---
  async function loadWorkers() {
    const tbody = el('workers-table-body');
    const metricWorkers = el('metric-workers-count');

    try {
      const res = await apiFetch('/api/v1/workers');
      const data = await res.json();
      cachedWorkers = Array.isArray(data) ? data : (data.workers || []);

      if (metricWorkers) metricWorkers.textContent = String(cachedWorkers.length);
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

    tbody.innerHTML = cachedWorkers.map(w => {
      const safeId = escapeHtml(w.id);
      const safeName = escapeHtml(w.name);
      return `
        <tr>
          <td style="font-family: var(--font-mono); font-weight: 600;">${safeId}</td>
          <td>
            <div style="font-weight: 600;">${safeName}</div>
            <div style="font-size: 0.75rem; color: var(--text-dim);">${safeName.startsWith('coordinator') ? 'Coordinateur avec agent intégré' : 'Agent distant HTTP'}</div>
          </td>
          <td>${formatDate(w.last_seen)}</td>
          <td>
            <span class="status-pill online">
              <span class="status-dot"></span>
              Enregistré
            </span>
          </td>
        </tr>
      `;
    }).join('');
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
  }
