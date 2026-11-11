// --- Outils et catalogue dynamiques ---
  async function loadToolsStatus() {
    try {
      const res = await apiFetch('/api/v1/tools');
      const tools = await res.json();
      if (!Array.isArray(tools)) return;
      tools.forEach(tool => {
        const card = document.querySelector(`.tool-card[data-tool-id="${tool.id}"]`);
        if (card) {
          card.setAttribute('data-cache-status', tool.status);
          const pill = card.querySelector('.status-pill');
          if (pill) {
            if (tool.status === 'ready') {
              pill.className = 'status-pill success';
              pill.title = 'Marqueur .ready validé et binaire intègre';
              pill.innerHTML = '<span class="status-dot"></span>.ready (Prêt)';
            } else if (tool.status === 'locked') {
              pill.className = 'status-pill warning';
              pill.title = 'Verrou .lock actif (acquisition en cours)';
              pill.innerHTML = '<span class="status-dot"></span>.lock (En cours)';
            } else {
              pill.className = 'status-pill';
              pill.title = 'Outil non extrait dans le cache worker';
              pill.innerHTML = '<span class="status-dot"></span>Non installé';
            }
          }
        }
      });
    } catch {}
  }

  // --- Gestion du catalogue d'outils et filtres ---
  function setupPluginsPanel() {
    // Filtrage des cartes d'outils
    document.querySelectorAll('.filter-chip[data-tool-filter]').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.filter-chip[data-tool-filter]').forEach(b => {
          b.setAttribute('aria-pressed', 'false');
          b.classList.remove('active');
        });
        btn.setAttribute('aria-pressed', 'true');
        btn.classList.add('active');

        const filter = btn.getAttribute('data-tool-filter') || 'all';
        document.querySelectorAll('.tool-card').forEach(card => {
          const status = card.getAttribute('data-cache-status');
          if (filter === 'all' || status === filter) {
            card.style.display = 'flex';
          } else {
            card.style.display = 'none';
          }
        });
      });
    });

    // Gestion du dialogue de configuration de plugin
    document.querySelectorAll('.open-plugin-config-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const pluginId = btn.getAttribute('data-plugin') || '';
        const pluginName = btn.getAttribute('data-plugin-name') || pluginId;
        const dialog = el('plugin-config-dialog');
        const titleEl = el('plugin-config-title');
        const hiddenId = el('plugin-config-current-id');
        const urlInput = el('plugin-cfg-url');
        const keyInput = el('plugin-cfg-apikey');
        const localInput = el('plugin-cfg-local-path');
        const remoteInput = el('plugin-cfg-remote-path');

        if (titleEl) titleEl.textContent = `Paramètres par défaut : ${pluginName}`;
        if (hiddenId) hiddenId.value = pluginId;

        try {
          const res = await apiFetch(`/api/v1/plugins/${encodeURIComponent(pluginId)}/settings`);
          if (res && res.settings && Object.keys(res.settings).length > 0) {
            if (urlInput) urlInput.value = res.settings.url || '';
            if (keyInput) keyInput.value = res.settings.apiKey || '';
            if (localInput) localInput.value = res.settings.pathMappingLocal || '/data';
            if (remoteInput) remoteInput.value = res.settings.pathMappingRemote || (pluginId.includes('sonarr') ? '/tv' : '/movies');
          } else {
            if (urlInput) urlInput.value = pluginId.includes('sonarr') ? 'http://192.168.1.50:8989' : 'http://192.168.1.50:7878';
            if (keyInput) keyInput.value = '';
            if (localInput) localInput.value = '/data';
            if (remoteInput) remoteInput.value = pluginId.includes('sonarr') ? '/tv' : '/movies';
          }
        } catch {
          if (urlInput) urlInput.value = pluginId.includes('sonarr') ? 'http://192.168.1.50:8989' : 'http://192.168.1.50:7878';
          if (keyInput) keyInput.value = '';
          if (localInput) localInput.value = '/data';
          if (remoteInput) remoteInput.value = pluginId.includes('sonarr') ? '/tv' : '/movies';
        }

        if (dialog && typeof dialog.showModal === 'function') {
          dialog.showModal();
        }
      });
    });

    const saveBtn = el('plugin-config-save-btn');
    if (saveBtn) {
      saveBtn.addEventListener('click', async () => {
        const pluginId = (el('plugin-config-current-id') && el('plugin-config-current-id').value) || '';
        if (!pluginId) return;

        const url = el('plugin-cfg-url') ? el('plugin-cfg-url').value.trim() : '';
        const apiKey = el('plugin-cfg-apikey') ? el('plugin-cfg-apikey').value.trim() : '';
        const pathMappingLocal = el('plugin-cfg-local-path') ? el('plugin-cfg-local-path').value.trim() : '/data';
        const pathMappingRemote = el('plugin-cfg-remote-path') ? el('plugin-cfg-remote-path').value.trim() : '/media';

        try {
          await apiFetch(`/api/v1/plugins/${encodeURIComponent(pluginId)}/settings`, {
            method: 'PUT',
            body: JSON.stringify({ url, apiKey, pathMappingLocal, pathMappingRemote }),
          });
          showNotice(`Paramètres par défaut pour ${pluginId} enregistrés ! Transmis aux flux.`, 'success');
          const dialog = el('plugin-config-dialog');
          if (dialog && dialog.open) dialog.close();
        } catch (e) {
          showNotice(`Erreur de sauvegarde : ${e.message || 'Échec API'}`, 'error');
        }
      });
    }

    const closeX = el('plugin-config-close-x');
    const cancelBtn = el('plugin-config-cancel-btn');
    if (closeX) closeX.addEventListener('click', () => { const d = el('plugin-config-dialog'); if (d) d.close(); });
    if (cancelBtn) cancelBtn.addEventListener('click', () => { const d = el('plugin-config-dialog'); if (d) d.close(); });
  }
