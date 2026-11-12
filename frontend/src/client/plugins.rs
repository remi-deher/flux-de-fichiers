//! Catalogue de plugins et dépôts, rendu dans le registre commun.

pub const SCRIPT: &str = r###"
// --- Outils et catalogue dynamiques ---
  let cachedToolStatus = [];
  const CATALOG_VIEWS = ['app', 'plugin', 'flow', 'block', 'pack', 'preset', 'resource'];

  function cssAttr(value) {
    const text = String(value || '');
    if (window.CSS && typeof CSS.escape === 'function') return CSS.escape(text);
    return text.replace(/["\\]/g, '\\$&');
  }

  function catalogViewOf(type) {
    const value = String(type || '').toLowerCase();
    if (value.indexOf('resource.') === 0) return 'resource';
    return CATALOG_VIEWS.indexOf(value) >= 0 ? value : '';
  }

  async function loadToolsStatus() {
    try {
      const res = await apiFetch('/api/v1/tools');
      const tools = await res.json();
      if (!Array.isArray(tools)) return;
      cachedToolStatus = tools;
      tools.forEach(tool => {
        const row = document.querySelector(`#catalog-body-app tr[data-app-id="${cssAttr(tool.id)}"]`);
        if (!row) return;
        row.setAttribute('data-cache-status', tool.status || '');
        const cell = row.querySelector('.tool-status');
        if (!cell) return;
        if (tool.status === 'ready') cell.textContent = 'Prêt';
        else if (tool.status === 'locked') cell.textContent = 'En cours';
        else cell.textContent = 'Absent';
      });
    } catch {}
  }

  function toolStatusOf(id) {
    const known = cachedToolStatus.find(tool => tool.id === id);
    if (!known) return '—';
    if (known.status === 'ready') return 'Prêt';
    if (known.status === 'locked') return 'En cours';
    return 'Absent';
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
        tbody.innerHTML = emptyStateRow(6, 'Aucun dépôt de plugins enregistré.', false);
        return;
      }

      tbody.innerHTML = cachedRepositories.map(repo => {
        const safeId = escapeHtml(repo.id);
        const safeName = escapeHtml(repo.name);
        const safeUrl = escapeHtml(repo.indexUrl);
        const isOfficial = !!repo.isOfficial;
        const isEnabled = !!repo.enabled;
        const lastSync = repo.lastSyncedAt ? escapeHtml(repo.lastSyncedAt.replace('T', ' ').slice(0, 19)) : 'Jamais';
        const officialBadge = renderStatusPill(isOfficial ? 'Officiel' : 'Tiers', isOfficial ? 'info' : 'default', {
          title: isOfficial ? 'Source certifiée par le projet' : 'Source externe tierce'
        });
        const statusBadge = renderStatusPill(isEnabled ? 'Actif' : 'Inactif', isEnabled ? 'success' : 'offline', {
          pulse: isEnabled
        });
        const deleteBtn = isOfficial
          ? `<button type="button" class="btn btn-secondary btn-sm" disabled title="Le dépôt officiel ne peut pas être supprimé">Conserver</button>`
          : `<button type="button" class="btn btn-danger btn-sm repo-delete-btn" data-repo-id="${safeId}" title="Supprimer ce dépôt">Supprimer</button>`;
        const toggleLabel = isEnabled ? 'Désactiver' : 'Activer';

        return `
          <tr class="registre-row" data-repo-id="${safeId}">
            <td>
              <div class="text-strong">${safeName}</div>
              <div class="text-xs text-mono">${safeId}</div>
            </td>
            <td><div class="truncate" title="${safeUrl}">${safeUrl}</div></td>
            <td>${officialBadge}</td>
            <td>${statusBadge}</td>
            <td class="text-sm text-dim">${lastSync}</td>
            <td class="cell-end">
              <div class="inline-actions">
                <button type="button" class="btn btn-secondary btn-sm repo-open-btn" data-repo-id="${safeId}">Ouvrir</button>
                <button type="button" class="btn btn-secondary btn-sm repo-toggle-btn" data-repo-id="${safeId}" title="${toggleLabel} ce dépôt">${toggleLabel}</button>
                ${deleteBtn}
              </div>
            </td>
          </tr>`;
      }).join('');

      tbody.querySelectorAll('.repo-open-btn').forEach(btn => {
        btn.addEventListener('click', () => {
          const id = btn.getAttribute('data-repo-id');
          const repo = cachedRepositories.find(item => item.id === id);
          if (!repo) return;
          markRegistreRow(tbody, btn.closest('tr'));
          const lastSync = repo.lastSyncedAt ? repo.lastSyncedAt.replace('T', ' ').slice(0, 19) : 'Jamais';
          showRegistreFact('repos-fact', repo.name || repo.id, [repo.indexUrl || '', lastSync]);
          const action = el('repos-fact-action');
          if (action) action.setAttribute('data-repo-id', repo.id || '');
        });
      });

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
      tbody.innerHTML = emptyStateRow(6, `Impossible de charger les dépôts : ${escapeHtml(e.message)}`, true);
    }
  }

  // --- Catalogue par nature ---
  let cachedStoreEntries = [];

  async function loadStoreCatalog() {
    try {
      const res = await apiFetch('/api/v1/plugins');
      const entries = await res.json();
      cachedStoreEntries = Array.isArray(entries) ? entries : [];
      renderCatalogRegistre();
    } catch (e) {
      CATALOG_VIEWS.forEach(view => {
        const tbody = el('catalog-body-' + view);
        if (tbody) {
          const cols = tbody.closest('table') ? tbody.closest('table').querySelectorAll('thead th').length : 3;
          tbody.innerHTML = emptyStateRow(cols, `Échec du chargement du catalogue : ${escapeHtml(e.message)}`, true);
        }
      });
    }
  }

  function catalogFacts(view, item) {
    const version = item.version || '';
    const capability = item.capability || '';
    const category = item.category || '';
    const tags = Array.isArray(item.tags) ? item.tags.filter(tag => typeof tag === 'string').join(', ') : '';
    if (view === 'app') return [version, capability, toolStatusOf(item.id)];
    if (view === 'plugin') return [version, tags];
    if (view === 'flow') return [version, capability, 'Pièce publiée'];
    if (view === 'block') return [version, category];
    if (view === 'resource') return [String(item.type || '')];
    return [version, String(item.type || '')];
  }

  function catalogVerb(view, item) {
    if (view === 'plugin') return 'Configurer';
    if (view === 'flow') return 'Importer';
    if (view === 'app') return toolStatusOf(item.id) === 'Prêt' ? 'Distribuer' : 'Installer';
    if (view === 'block') return 'Voir le bloc';
    if (view === 'pack') return 'Voir le pack';
    if (view === 'preset') return 'Voir le préréglage';
    if (view === 'resource') return 'Voir la ressource';
    return 'Ouvrir';
  }

  function catalogCells(view, item) {
    const name = escapeHtml(item.name || item.id || '');
    const version = escapeHtml(item.version || '');
    const capability = escapeHtml(item.capability || '—');
    const category = escapeHtml(item.category || '—');
    const type = escapeHtml(String(item.type || ''));
    const tags = Array.isArray(item.tags) ? item.tags.filter(tag => typeof tag === 'string') : [];
    const tagText = escapeHtml(tags.join(', ') || '—');
    if (view === 'app') {
      return `<td class="text-strong">${name}</td><td>${version}</td><td>${capability}</td><td class="tool-status">${escapeHtml(toolStatusOf(item.id))}</td>`;
    }
    if (view === 'plugin') return `<td class="text-strong">${name}</td><td>${version}</td><td>${tagText}</td>`;
    if (view === 'flow') return `<td class="text-strong">${name}</td><td>${version}</td><td>${capability}</td>`;
    if (view === 'block') return `<td class="text-strong">${name}</td><td>${version}</td><td>${category}</td>`;
    if (view === 'resource') return `<td class="text-strong">${name}</td><td>${type}</td>`;
    return `<td class="text-strong">${name}</td><td>${version}</td><td>${type}</td>`;
  }

  function renderCatalogRegistre() {
    CATALOG_VIEWS.forEach(view => {
      const tbody = el('catalog-body-' + view);
      if (!tbody) return;
      const rows = cachedStoreEntries.filter(entry => catalogViewOf(entry.type) === view);
      const cols = tbody.closest('table') ? tbody.closest('table').querySelectorAll('thead th').length : 3;
      if (rows.length === 0) {
        tbody.innerHTML = emptyStateRow(cols, 'Aucune pièce de cette nature dans le catalogue.', false);
        return;
      }
      tbody.innerHTML = rows.map(item => {
        const safeId = escapeHtml(item.id || '');
        const tags = Array.isArray(item.tags) ? item.tags.filter(tag => typeof tag === 'string').join(' ') : '';
        const toolAttr = view === 'app' ? ` data-app-id="${safeId}"` : '';
        return `<tr class="registre-row" data-catalog-id="${safeId}" data-tags="${escapeHtml(tags)}"${toolAttr}>
          ${catalogCells(view, item)}
          <td class="cell-end"><button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-catalog-id="${safeId}">Ouvrir</button></td>
        </tr>`;
      }).join('');
      tbody.querySelectorAll('.registre-open-btn').forEach(btn => {
        btn.addEventListener('click', () => openCatalogRow(view, btn.getAttribute('data-catalog-id')));
      });
    });
  }

  function openCatalogRow(view, id) {
    const item = cachedStoreEntries.find(entry => entry.id === id);
    const tbody = el('catalog-body-' + view);
    if (!item || !tbody) return;
    markRegistreRow(tbody, tbody.querySelector(`tr[data-catalog-id="${cssAttr(id)}"]`));
    showRegistreFact('catalog-fact-' + view, item.name || item.id, catalogFacts(view, item));
    const action = el('catalog-fact-' + view + '-action');
    if (!action) return;
    const verb = catalogVerb(view, item);
    action.textContent = verb;
    const verbTab = el('catalog-fact-' + view + '-tab-verb');
    if (verbTab) verbTab.textContent = verb;
    action.setAttribute('data-catalog-id', item.id || '');
    action.setAttribute('data-catalog-name', item.name || item.id || '');
    action.setAttribute('data-verb', catalogVerb(view, item));
  }

  async function openPluginSettings(pluginId, pluginName) {
    const dialog = el('plugin-config-dialog');
    const titleEl = el('plugin-config-title');
    const hiddenId = el('plugin-config-current-id');
    if (titleEl) titleEl.textContent = `Paramètres par défaut : ${pluginName}`;
    if (hiddenId) hiddenId.value = pluginId;
    try {
      const res = await apiFetch(`/api/v1/plugins/${encodeURIComponent(pluginId)}/settings`);
      const body = await res.json();
      renderPluginConfigFields(body.fields || []);
    } catch (e) {
      renderPluginConfigMessage('Impossible de lire les paramètres.');
      showNotice(`Impossible de lire les paramètres : ${e.message || 'Échec API'}`, 'error');
    }
    if (dialog && typeof dialog.showModal === 'function') dialog.showModal();
  }

  async function importCatalogFlow(flowId, btn) {
    const previous = btn.textContent;
    btn.disabled = true;
    btn.textContent = 'Import…';
    try {
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
      showNotice(`Flux ${templateTitle} importé dans vos flux.`, 'success');
      btn.textContent = 'Importé';
      if (typeof loadFlows === 'function') loadFlows();
    } catch (e) {
      btn.disabled = false;
      btn.textContent = previous;
      showNotice(`Erreur d'import : ${e.message}`, 'error');
    }
  }

  function settingInputType(fieldType) {
    if (fieldType === 'secret') return 'password';
    if (fieldType === 'url') return 'url';
    return 'text';
  }

  function renderPluginConfigMessage(message) {
    const container = el('plugin-config-fields');
    if (!container) return;
    container.innerHTML = `<p class="empty-state-text">${escapeHtml(message)}</p>`;
  }

  function renderPluginConfigFields(fields) {
    const container = el('plugin-config-fields');
    if (!container) return;
    if (!Array.isArray(fields) || fields.length === 0) {
      renderPluginConfigMessage('Ce plugin ne déclare aucun paramètre.');
      return;
    }
    container.innerHTML = fields.map(field => {
      const name = String(field.name || '');
      const inputId = 'plugin-cfg-' + name;
      const value = field.value == null ? '' : String(field.value);
      const help = field.description
        ? `<p class="form-help">${escapeHtml(String(field.description))}</p>`
        : '';
      const label = String(field.label || name) + (field.required ? ' *' : '');
      return `<div class="form-group">
        <label class="form-label" for="${escapeHtml(inputId)}">${escapeHtml(label)}</label>
        <input class="form-input" type="${settingInputType(field.type)}" id="${escapeHtml(inputId)}" data-setting-name="${escapeHtml(name)}" value="${escapeHtml(value)}" autocomplete="off" />
        ${help}
      </div>`;
    }).join('');
  }

  function setupPluginsPanel() {
    CATALOG_VIEWS.forEach(view => {
      const action = el('catalog-fact-' + view + '-action');
      if (!action) return;
      action.addEventListener('click', async () => {
        const verb = action.getAttribute('data-verb') || '';
        const id = action.getAttribute('data-catalog-id') || '';
        const name = action.getAttribute('data-catalog-name') || id;
        if (!id) {
          showNotice('Ouvrez une ligne du catalogue.', 'info');
          return;
        }
        if (verb === 'Configurer') {
          await openPluginSettings(id, name);
          return;
        }
        if (verb === 'Importer') {
          await importCatalogFlow(id, action);
          return;
        }
        if (verb === 'Distribuer' || verb === 'Installer') {
          showNotice(`${verb} : ${name}. Le cache du nœud porte le binaire.`, 'info');
          return;
        }
        const fact = el('catalog-fact-' + view);
        if (fact) fact.scrollIntoView({ block: 'nearest' });
      });
    });

    const reposAction = el('repos-fact-action');
    if (reposAction) {
      reposAction.addEventListener('click', async () => {
        const id = reposAction.getAttribute('data-repo-id') || '';
        if (!id) {
          showNotice('Ouvrez un dépôt avant de le synchroniser.', 'info');
          return;
        }
        try {
          reposAction.disabled = true;
          await apiFetch(`/api/v1/plugins/repositories/${encodeURIComponent(id)}/sync`, { method: 'POST' });
          showNotice('Dépôt synchronisé.', 'success');
          await loadRepositories();
          await loadStoreCatalog();
        } catch (e) {
          showNotice(`Erreur de synchronisation : ${e.message}`, 'error');
        } finally {
          reposAction.disabled = false;
        }
      });
    }

    const saveBtn = el('plugin-config-save-btn');
    if (saveBtn) {
      saveBtn.addEventListener('click', async () => {
        const pluginId = (el('plugin-config-current-id') && el('plugin-config-current-id').value) || '';
        if (!pluginId) return;
        const fieldsRoot = el('plugin-config-fields');
        const inputs = fieldsRoot ? fieldsRoot.querySelectorAll('[data-setting-name]') : [];
        if (!inputs.length) {
          showNotice('Ce plugin ne déclare aucun paramètre à enregistrer.', 'error');
          return;
        }
        const payload = {};
        inputs.forEach(input => {
          const key = input.getAttribute('data-setting-name');
          if (key) payload[key] = input.value.trim();
        });
        try {
          await apiFetch(`/api/v1/plugins/${encodeURIComponent(pluginId)}/settings`, {
            method: 'PUT',
            body: JSON.stringify(payload),
          });
          showNotice(`Paramètres par défaut pour ${pluginId} enregistrés. Transmis aux flux.`, 'success');
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
        if (typeof addRepoDialog.showModal === 'function') addRepoDialog.showModal();
        else addRepoDialog.setAttribute('open', '');
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
          showNotice(`Dépôt "${name}" ajouté.`, 'success');
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

    window.syncRepositoriesSilently = async function() {
      if (!Array.isArray(cachedRepositories) || cachedRepositories.length === 0) return;
      for (const repo of cachedRepositories) {
        if (repo.enabled !== false) {
          try {
            await apiFetch(`/api/v1/plugins/repositories/${encodeURIComponent(repo.id)}/sync`, { method: 'POST' });
          } catch {}
        }
      }
      await loadRepositories();
      await loadStoreCatalog();
    };

    document.addEventListener('flux:subnav-change', (e) => {
      const targetId = e.detail && e.detail.targetId;
      if (!targetId || targetId.indexOf('subtab-plugins-') !== 0) return;
      if (targetId === 'subtab-plugins-repos') loadRepositories();
      else if (targetId === 'subtab-plugins-app') loadToolsStatus();
      else renderCatalogRegistre();
    });

    loadRepositories();
  }

"###;
