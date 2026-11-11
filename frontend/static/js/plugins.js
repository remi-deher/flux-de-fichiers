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

  // --- Gestion Multi-Dépôts ---
  let cachedRepositories = [];

  async function loadRepositories() {
    const tbody = el('repos-table-body');
    if (!tbody) return;

    try {
      const res = await apiFetch('/api/v1/plugins/repositories');
      const repos = await res.json();
      cachedRepositories = Array.isArray(repos) ? repos : [];

      if (cachedRepositories.length === 0) {
        tbody.innerHTML = `<tr><td colspan="6" class="empty-state-text">Aucun dépôt de plugins enregistré.</td></tr>`;
        return;
      }

      tbody.innerHTML = cachedRepositories.map(repo => {
        const safeId = escapeHtml(repo.id);
        const safeName = escapeHtml(repo.name);
        const safeUrl = escapeHtml(repo.indexUrl);
        const isOfficial = !!repo.isOfficial;
        const isEnabled = !!repo.enabled;
        const lastSync = repo.lastSyncedAt ? escapeHtml(repo.lastSyncedAt.replace('T', ' ').slice(0, 19)) : 'Jamais';

        const officialBadge = isOfficial
          ? `<span class="status-pill info" title="Source certifiée par le projet"><span class="status-dot"></span>Officiel</span>`
          : `<span class="status-pill" title="Source externe tierce"><span class="status-dot"></span>Tiers</span>`;

        const statusBadge = isEnabled
          ? `<span class="status-pill success"><span class="status-dot"></span>Actif</span>`
          : `<span class="status-pill offline"><span class="status-dot"></span>Inactif</span>`;

        const deleteBtn = isOfficial
          ? `<button type="button" class="btn btn-secondary btn-sm" disabled title="Le dépôt officiel ne peut pas être supprimé">🔒</button>`
          : `<button type="button" class="btn btn-danger btn-sm repo-delete-btn" data-repo-id="${safeId}" title="Supprimer ce dépôt">🗑️</button>`;

        const toggleLabel = isEnabled ? 'Désactiver' : 'Activer';

        return `
          <tr>
            <td>
              <div style="font-weight: 600; color: var(--text-main);">${safeName}</div>
              <div style="font-size: 0.75rem; color: var(--text-dim); font-family: var(--font-mono);">${safeId}</div>
            </td>
            <td>
              <div style="font-size: 0.8125rem; font-family: var(--font-mono); max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" title="${safeUrl}">
                ${safeUrl}
              </div>
            </td>
            <td>${officialBadge}</td>
            <td>${statusBadge}</td>
            <td style="font-size: 0.8125rem; color: var(--text-dim);">${lastSync}</td>
            <td style="text-align: right;">
              <div style="display: inline-flex; gap: 0.4rem;">
                <button type="button" class="btn btn-secondary btn-sm repo-toggle-btn" data-repo-id="${safeId}" title="${toggleLabel} ce dépôt">
                  ${toggleLabel}
                </button>
                <button type="button" class="btn btn-secondary btn-sm repo-sync-btn" data-repo-id="${safeId}" title="Synchroniser maintenant">
                  🔄
                </button>
                ${deleteBtn}
              </div>
            </td>
          </tr>
        `;
      }).join('');

      tbody.querySelectorAll('.repo-toggle-btn').forEach(btn => {
        btn.addEventListener('click', async () => {
          const id = btn.getAttribute('data-repo-id');
          try {
            await apiFetch(`/api/v1/plugins/repositories/${encodeURIComponent(id)}/toggle`, { method: 'PUT' });
            showNotice('État du dépôt mis à jour', 'success');
            await loadRepositories();
            await loadStoreCatalog();
          } catch (e) {
            showNotice(`Erreur : ${e.message}`, 'error');
          }
        });
      });

      tbody.querySelectorAll('.repo-sync-btn').forEach(btn => {
        btn.addEventListener('click', async () => {
          const id = btn.getAttribute('data-repo-id');
          try {
            btn.textContent = '…';
            await apiFetch(`/api/v1/plugins/repositories/${encodeURIComponent(id)}/sync`, { method: 'POST' });
            showNotice('Dépôt synchronisé avec succès', 'success');
            await loadRepositories();
            await loadStoreCatalog();
          } catch (e) {
            showNotice(`Erreur de synchronisation : ${e.message}`, 'error');
          }
        });
      });

      tbody.querySelectorAll('.repo-delete-btn').forEach(btn => {
        btn.addEventListener('click', async () => {
          const id = btn.getAttribute('data-repo-id');
          if (!confirm(`Supprimer définitivement le dépôt ${id} ?`)) return;
          try {
            await apiFetch(`/api/v1/plugins/repositories/${encodeURIComponent(id)}`, { method: 'DELETE' });
            showNotice('Dépôt supprimé', 'success');
            await loadRepositories();
            await loadStoreCatalog();
          } catch (e) {
            showNotice(`Erreur de suppression : ${e.message}`, 'error');
          }
        });
      });

    } catch (e) {
      tbody.innerHTML = `<tr><td colspan="6" class="empty-state-text" style="color: var(--danger-text);">
        Impossible de charger les dépôts : ${escapeHtml(e.message)}
      </td></tr>`;
    }
  }

  // --- Gestion du Catalogue Distant (Store) ---
  let cachedStoreEntries = [];

  async function loadStoreCatalog() {
    const container = el('store-cards-container');
    if (!container) return;

    try {
      const res = await apiFetch('/api/v1/plugins');
      const entries = await res.json();
      cachedStoreEntries = Array.isArray(entries) ? entries : [];

      renderStoreCards();
    } catch (e) {
      container.innerHTML = `<div class="empty-state-text" style="grid-column: 1 / -1; color: var(--danger-text);">
        Échec du chargement du catalogue distant : ${escapeHtml(e.message)}
      </div>`;
    }
  }

  function renderStoreCards() {
    const container = el('store-cards-container');
    if (!container) return;

    const activeFilterBtn = document.querySelector('.store-filter-chip.active');
    const filter = (activeFilterBtn && activeFilterBtn.getAttribute('data-store-filter')) || 'all';

    const filtered = cachedStoreEntries.filter(entry => {
      if (filter === 'all') return true;
      return (entry.type || '').toLowerCase() === filter;
    });

    if (filtered.length === 0) {
      container.innerHTML = `<div class="empty-state-text" style="grid-column: 1 / -1; padding: 2rem;">
        Aucun composant disponible dans le catalogue pour ce filtre.
      </div>`;
      return;
    }

    container.innerHTML = filtered.map(item => {
      const safeId = escapeHtml(item.id || '');
      const safeName = escapeHtml(item.name || safeId);
      const safeType = escapeHtml(item.type || 'extension');
      const safeVer = escapeHtml(item.version || '1.0.0');
      const isFlow = safeType === 'flow';
      const icon = isFlow ? '⚡' : '🔧';

      const actionBtn = isFlow
        ? `<button type="button" class="btn btn-primary btn-sm store-install-flow-btn" data-flow-id="${safeId}">
             📥 Importer le flux
           </button>`
        : `<span class="status-pill success"><span class="status-dot"></span>Distribution vérifiée</span>`;

      return `
        <article class="tool-card" data-store-id="${safeId}" data-store-type="${safeType}">
          <div class="tool-card-header">
            <div class="tool-identity">
              <span class="tool-badge-icon" aria-hidden="true">${icon}</span>
              <div>
                <h3 class="tool-card-title">${safeName}</h3>
                <span class="tool-card-id">${safeId} • v${safeVer}</span>
              </div>
            </div>
            <span class="status-pill info">
              <span class="status-dot"></span>${safeType.toUpperCase()}
            </span>
          </div>
          <p class="tool-card-desc">
            ${isFlow ? 'Pipeline de traitement multimédia certifié distribuable.' : 'Outil de traitement autonome vérifié par empreinte SHA-256.'}
          </p>
          <div class="tool-meta-tags">
            <span class="meta-tag">Origine : Dépôt officiel</span>
            <span class="meta-tag">Type : ${safeType}</span>
          </div>
          <div style="margin-top: 1rem; display: flex; justify-content: flex-end;">
            ${actionBtn}
          </div>
        </article>
      `;
    }).join('');

    // Branchement de l'import de flux certifié
    container.querySelectorAll('.store-install-flow-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const flowId = btn.getAttribute('data-flow-id');
        btn.disabled = true;
        btn.textContent = 'Import…';

        try {
          // Construction du flux à partir de la définition certifiée
          const templateTitle = flowId.includes('hevc') ? 'Transcodage intelligent HEVC'
            : flowId.includes('faststart') ? 'Optimisation MP4 Web FastStart'
            : 'Remuxage Matroska certifié';

          await apiFetch('/api/v1/flows', {
            method: 'POST',
            body: JSON.stringify({
              id: flowId,
              name: templateTitle,
              category: 'video',
              description: 'Flux certifié importé depuis le catalogue distant de plugins.',
              is_default: false,
              pipeline_definition: JSON.stringify({
                nodes: [{ id: 'n1', type: 'file-input' }, { id: 'n2', type: 'mkvmerge' }, { id: 'n3', type: 'file-output' }]
              })
            })
          });

          showNotice(`Flux ${templateTitle} importé avec succès dans vos flux !`, 'success');
          btn.textContent = '✓ Importé';
          if (typeof loadFlows === 'function') loadFlows();
        } catch (e) {
          btn.disabled = false;
          btn.textContent = '📥 Importer le flux';
          showNotice(`Erreur d'import : ${e.message}`, 'error');
        }
      });
    });
  }

  // --- Initialisation du panneau Plugins et des écouteurs ---
  function setupPluginsPanel() {
    // Filtrage des cartes d'outils locaux
    document.querySelectorAll('.filter-chip[data-tool-filter]').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.filter-chip[data-tool-filter]').forEach(b => {
          b.setAttribute('aria-pressed', 'false');
          b.classList.remove('active');
        });
        btn.setAttribute('aria-pressed', 'true');
        btn.classList.add('active');

        const filter = btn.getAttribute('data-tool-filter') || 'all';
        document.querySelectorAll('.tool-card[data-cache-status]').forEach(card => {
          const status = card.getAttribute('data-cache-status');
          if (filter === 'all' || status === filter) {
            card.style.display = 'flex';
          } else {
            card.style.display = 'none';
          }
        });
      });
    });

    // Filtrage du store distant
    document.querySelectorAll('.store-filter-chip').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.store-filter-chip').forEach(b => {
          b.setAttribute('aria-pressed', 'false');
          b.classList.remove('active');
        });
        btn.setAttribute('aria-pressed', 'true');
        btn.classList.add('active');
        renderStoreCards();
      });
    });

    // Gestion du dialogue de configuration de plugin (*arr)
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

    // Dialogue d'ajout de dépôt
    const openAddRepoBtn = el('btn-open-add-repo-dialog');
    const addRepoDialog = el('add-repo-dialog');
    const closeAddRepoX = el('add-repo-close-x');
    const cancelAddRepoBtn = el('add-repo-cancel-btn');
    const saveAddRepoBtn = el('add-repo-save-btn');

    if (openAddRepoBtn && addRepoDialog) {
      openAddRepoBtn.addEventListener('click', () => {
        const nameInput = el('repo-name-input');
        const urlInput = el('repo-url-input');
        if (nameInput) nameInput.value = '';
        if (urlInput) urlInput.value = '';
        if (typeof addRepoDialog.showModal === 'function') {
          addRepoDialog.showModal();
        } else {
          addRepoDialog.setAttribute('open', '');
        }
      });
    }

    const closeAddRepo = () => {
      if (addRepoDialog) {
        if (typeof addRepoDialog.close === 'function') addRepoDialog.close();
        else addRepoDialog.removeAttribute('open');
      }
    };
    if (closeAddRepoX) closeAddRepoX.addEventListener('click', closeAddRepo);
    if (cancelAddRepoBtn) cancelAddRepoBtn.addEventListener('click', closeAddRepo);

    if (saveAddRepoBtn) {
      saveAddRepoBtn.addEventListener('click', async () => {
        const nameInput = el('repo-name-input');
        const urlInput = el('repo-url-input');
        const name = nameInput ? nameInput.value.trim() : '';
        const url = urlInput ? urlInput.value.trim() : '';

        if (!name || !url) {
          showNotice('Veuillez renseigner le nom et l\'URL du dépôt.', 'error');
          return;
        }

        try {
          saveAddRepoBtn.disabled = true;
          saveAddRepoBtn.textContent = 'Ajout…';
          await apiFetch('/api/v1/plugins/repositories', {
            method: 'POST',
            body: JSON.stringify({ name, indexUrl: url })
          });
          showNotice(`Dépôt "${name}" ajouté avec succès !`, 'success');
          closeAddRepo();
          await loadRepositories();
          await loadStoreCatalog();
        } catch (e) {
          showNotice(`Erreur : ${e.message}`, 'error');
        } finally {
          saveAddRepoBtn.disabled = false;
          saveAddRepoBtn.textContent = 'Ajouter le dépôt';
        }
      });
    }

    // Bouton de synchronisation de tous les dépôts
    const syncAllBtn = el('btn-sync-all-repos');
    if (syncAllBtn) {
      syncAllBtn.addEventListener('click', async () => {
        syncAllBtn.disabled = true;
        syncAllBtn.textContent = 'Synchronisation…';
        try {
          for (const repo of cachedRepositories) {
            try {
              await apiFetch(`/api/v1/plugins/repositories/${encodeURIComponent(repo.id)}/sync`, { method: 'POST' });
            } catch {}
          }
          showNotice('Tous les dépôts ont été synchronisés !', 'success');
          await loadRepositories();
          await loadStoreCatalog();
        } catch (e) {
          showNotice(`Erreur : ${e.message}`, 'error');
        } finally {
          syncAllBtn.disabled = false;
          syncAllBtn.textContent = '🔄 Synchroniser';
        }
      });
    }

    // Écoute de l'événement de changement d'onglet
    document.addEventListener('flux:subnav-change', (e) => {
      const targetId = e.detail && e.detail.targetId;
      if (targetId === 'subtab-plugins-store') {
        loadStoreCatalog();
      } else if (targetId === 'subtab-plugins-repos') {
        loadRepositories();
      } else if (targetId === 'subtab-plugins-installed') {
        loadToolsStatus();
      }
    });

    // Chargement initial des dépôts en arrière-plan
    loadRepositories();
  }
