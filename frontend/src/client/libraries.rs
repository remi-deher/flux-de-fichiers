//! Liste des bibliothèques déclarées et édition dans le tiroir.

pub const SCRIPT: &str = r###"
  let cachedLibraries = [];
  let libraryDraft = null;

  function libraryFlowName(flowId) {
    if (!flowId) return 'Aucun flux';
    const flow = cachedFlows.find(item => item.id === flowId);
    return flow ? (flow.name || flow.id) : flowId;
  }

  function libraryReadiness(library) {
    const workers = library && Array.isArray(library.workers) ? library.workers : [];
    return library && library.flow_id && workers.length > 0 ? 'Prête' : 'À configurer';
  }

  function libraryAccessLabel(access) {
    return access === 'read' ? 'Lecture seule' : 'Lecture / Écriture';
  }

  function libraryWorker(workerId) {
    return cachedWorkers.find(worker => worker.id === workerId) || null;
  }

  function libraryErrorMessage(error) {
    const code = error && error.message;
    if (code === 'Non autorisé') return 'Ouvrez une session pour modifier les bibliothèques.';
    if (code === 'lease_expired_or_conflict') return 'Une bibliothèque porte déjà ce nom.';
    if (code === 'invalid_input') return 'Vérifiez le nom, le flux et les chemins absolus des workers.';
    if (code === 'not_found') return 'Cette bibliothèque n’existe plus.';
    return 'Enregistrement impossible.';
  }

  async function loadLibraries() {
    const tbody = el('libraries-table-body');
    try {
      const res = await apiFetch('/api/v1/libraries');
      const data = await res.json();
      cachedLibraries = Array.isArray(data) ? data : [];
      renderLibrariesTable();
      markWidgetSource('libraries', true);
    } catch (e) {
      markWidgetSource('libraries', false);
      if (tbody) {
        const message = e.message === 'Non autorisé'
          ? 'Ouvrez une session pour afficher les bibliothèques.'
          : 'Impossible de récupérer les bibliothèques.';
        tbody.innerHTML = emptyStateRow(5, escapeHtml(message), true);
      }
    }
  }

  function librarySearchParts(library) {
    const workers = Array.isArray(library.workers) ? library.workers : [];
    return [
      library.name,
      libraryFlowName(library.flow_id),
      ...workers.map(entry => entry.local_path),
      ...workers.map(entry => {
        const worker = libraryWorker(entry.worker_id);
        return worker ? worker.name : '';
      })
    ];
  }

  function renderLibrariesTable() {
    const tbody = el('libraries-table-body');
    const searchInput = el('libraries-search-input');
    if (!tbody) return;
    const filtered = cachedLibraries.filter(library => matchesQuery(librarySearchParts(library), searchInput ? searchInput.value : ''));
    if (filtered.length === 0) {
      const message = cachedLibraries.length === 0
        ? 'Aucune bibliothèque déclarée. Déclarez-en une pour y accrocher un flux et des workers.'
        : 'Aucune bibliothèque ne correspond à votre recherche.';
      tbody.innerHTML = emptyStateRow(5, escapeHtml(message), false);
      return;
    }
    tbody.innerHTML = filtered.map(library => {
      const safeId = escapeHtml(library.id);
      const workers = Array.isArray(library.workers) ? library.workers.length : 0;
      const ready = libraryReadiness(library) === 'Prête';
      return `<tr class="registre-row" data-library-id="${safeId}">
          <td class="text-strong">${escapeHtml(library.name)}</td>
          <td>${escapeHtml(libraryFlowName(library.flow_id))}</td>
          <td>${workers}</td>
          <td>${renderStatusPill(libraryReadiness(library), ready ? 'online' : 'info')}</td>
          <td class="cell-end">
            <button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-library-id="${safeId}">Ouvrir</button>
          </td>
      </tr>`;
    }).join('');
    if (libraryDraft && libraryDraft.id) {
      const openRow = tbody.querySelector(`tr[data-library-id="${libraryDraft.id}"]`);
      if (openRow) openRow.classList.add('is-open');
    }
    tbody.querySelectorAll('.registre-open-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const id = btn.getAttribute('data-library-id');
        const library = cachedLibraries.find(item => item.id === id);
        if (!library) return;
        markRegistreRow(tbody, btn.closest('tr'));
        openLibrary(library);
      });
    });
  }

  function openLibrary(library) {
    const workers = library && Array.isArray(library.workers) ? library.workers : [];
    libraryDraft = {
      id: library ? library.id : null,
      name: library ? library.name : '',
      flow_id: library && library.flow_id ? library.flow_id : null,
      workers: workers.map(entry => ({
        worker_id: entry.worker_id,
        local_path: entry.local_path,
        access: entry.access === 'read' ? 'read' : 'read_write'
      }))
    };
    const title = libraryDraft.name || 'Nouvelle bibliothèque';
    const facts = library
      ? [libraryFlowName(library.flow_id), `${workers.length} worker(s)`, libraryReadiness(library)]
      : ['Choisissez un nom, un flux et au moins un worker.'];
    showRegistreFact('libraries-fact', title, facts);
    fillLibraryDrawer();
  }

  function fillLibraryDrawer() {
    if (!libraryDraft) return;
    const nameInput = el('library-name-input');
    if (nameInput) nameInput.value = libraryDraft.name || '';
    const deleteBtn = el('libraries-delete-btn');
    if (deleteBtn) deleteBtn.hidden = !libraryDraft.id;
    renderLibraryWorkers();
    renderLibraryWorkerSelect();
    renderLibraryFlowSelect();
  }

  function renderLibraryWorkers() {
    const list = el('library-workers-list');
    if (!list || !libraryDraft) return;
    if (libraryDraft.workers.length === 0) {
      list.innerHTML = '<p class="inspection-panel__facts">Aucun worker configuré.</p>';
      return;
    }
    const now = Date.now();
    list.innerHTML = libraryDraft.workers.map(entry => {
      const worker = libraryWorker(entry.worker_id);
      const online = worker && (now - new Date(worker.last_seen).getTime()) < 120000;
      const presence = worker
        ? renderStatusPill(online ? 'En ligne' : 'Hors ligne', online ? 'online' : 'offline')
        : renderStatusPill('Inconnu', 'info');
      const safeId = escapeHtml(entry.worker_id);
      return `<div class="library-entry-row">
          <div>
            <div class="text-strong">${escapeHtml(worker ? worker.name : entry.worker_id)}</div>
            <div class="library-entry-path">${escapeHtml(entry.local_path)}</div>
          </div>
          <div class="inline-actions">
            ${presence}
            <span class="status-pill info">${escapeHtml(libraryAccessLabel(entry.access))}</span>
            <button type="button" class="btn btn-secondary btn-sm library-entry-remove" data-worker-id="${safeId}">Retirer</button>
          </div>
      </div>`;
    }).join('');
    list.querySelectorAll('.library-entry-remove').forEach(btn => {
      btn.addEventListener('click', () => removeLibraryWorker(btn.getAttribute('data-worker-id')));
    });
  }

  function renderLibraryWorkerSelect() {
    const select = el('library-entry-worker');
    const addBtn = el('library-entry-add-btn');
    if (!select || !libraryDraft) return;
    const used = new Set(libraryDraft.workers.map(entry => entry.worker_id));
    const available = cachedWorkers.filter(worker => !used.has(worker.id));
    if (available.length === 0) {
      select.innerHTML = '<option value="">Aucun worker disponible</option>';
      select.disabled = true;
      if (addBtn) addBtn.disabled = true;
      return;
    }
    select.disabled = false;
    if (addBtn) addBtn.disabled = false;
    select.innerHTML = available.map(worker => {
      return `<option value="${escapeHtml(worker.id)}">${escapeHtml(worker.name || worker.id)}</option>`;
    }).join('');
  }

  function renderLibraryFlowSelect() {
    const select = el('library-flow-select');
    if (!select || !libraryDraft) return;
    const current = libraryDraft.flow_id || '';
    const options = ['<option value="">Aucun flux</option>'].concat(cachedFlows.map(flow => {
      return `<option value="${escapeHtml(flow.id)}">${escapeHtml(flow.name || flow.id)}</option>`;
    }));
    select.innerHTML = options.join('');
    select.value = current;
  }

  function removeLibraryWorker(workerId) {
    if (!libraryDraft || !workerId) return;
    libraryDraft.workers = libraryDraft.workers.filter(entry => entry.worker_id !== workerId);
    renderLibraryWorkers();
    renderLibraryWorkerSelect();
  }

  function addLibraryWorker() {
    if (!libraryDraft) return;
    const workerSelect = el('library-entry-worker');
    const pathInput = el('library-entry-path');
    const accessSelect = el('library-entry-access');
    const workerId = workerSelect ? workerSelect.value : '';
    const path = pathInput ? pathInput.value.trim() : '';
    const access = accessSelect && accessSelect.value === 'read' ? 'read' : 'read_write';
    if (!workerId || !path.startsWith('/')) {
      showNotice('Choisissez un worker et un chemin absolu.', 'info');
      return;
    }
    if (libraryDraft.workers.some(entry => entry.worker_id === workerId)) return;
    libraryDraft.workers.push({ worker_id: workerId, local_path: path, access });
    if (pathInput) pathInput.value = '';
    renderLibraryWorkers();
    renderLibraryWorkerSelect();
  }

  async function saveLibrary() {
    if (!libraryDraft) return;
    const nameInput = el('library-name-input');
    const flowSelect = el('library-flow-select');
    const payload = {
      name: nameInput ? nameInput.value : libraryDraft.name,
      flow_id: flowSelect && flowSelect.value ? flowSelect.value : null,
      workers: libraryDraft.workers
    };
    const creating = !libraryDraft.id;
    const url = creating ? '/api/v1/libraries' : `/api/v1/libraries/${libraryDraft.id}`;
    try {
      const res = await apiFetch(url, {
        method: creating ? 'POST' : 'PUT',
        body: JSON.stringify(payload)
      });
      const saved = await res.json();
      showNotice(creating ? 'Bibliothèque déclarée.' : 'Bibliothèque enregistrée.', 'success');
      await loadLibraries();
      const tbody = el('libraries-table-body');
      const row = tbody ? tbody.querySelector(`tr[data-library-id="${saved.id}"]`) : null;
      markRegistreRow(tbody, row);
      openLibrary(saved);
    } catch (e) {
      showNotice(libraryErrorMessage(e), 'error');
    }
  }

  async function deleteLibrary() {
    if (!libraryDraft || !libraryDraft.id) return;
    if (!confirm('Supprimer cette bibliothèque ?')) return;
    try {
      await apiFetch(`/api/v1/libraries/${libraryDraft.id}`, { method: 'DELETE' });
      libraryDraft = null;
      closeInspectionDrawer(el('libraries-fact'));
      showNotice('Bibliothèque supprimée.', 'success');
      await loadLibraries();
    } catch (e) {
      showNotice(libraryErrorMessage(e), 'error');
    }
  }

  function setupLibraries() {
    const search = el('libraries-search-input');
    if (search) {
      search.addEventListener('input', () => {
        const globalSearchInput = el('global-search-input');
        if (globalSearchInput && globalSearchInput.value !== search.value) {
          globalSearchInput.value = search.value;
        }
        renderLibrariesTable();
      });
    }
    const createBtn = el('library-create-btn');
    if (createBtn) {
      createBtn.addEventListener('click', () => {
        const tbody = el('libraries-table-body');
        if (tbody) markRegistreRow(tbody, null);
        openLibrary(null);
      });
    }
    const saveBtn = el('libraries-save-btn');
    if (saveBtn) saveBtn.addEventListener('click', saveLibrary);
    const deleteBtn = el('libraries-delete-btn');
    if (deleteBtn) deleteBtn.addEventListener('click', deleteLibrary);
    const addBtn = el('library-entry-add-btn');
    if (addBtn) addBtn.addEventListener('click', addLibraryWorker);
    const nameInput = el('library-name-input');
    if (nameInput) {
      nameInput.addEventListener('input', () => {
        if (!libraryDraft) return;
        libraryDraft.name = nameInput.value;
        const title = el('libraries-fact-title');
        if (title) title.textContent = nameInput.value.trim() || 'Nouvelle bibliothèque';
      });
    }
    const flowSelect = el('library-flow-select');
    if (flowSelect) {
      flowSelect.addEventListener('change', () => {
        if (!libraryDraft) return;
        libraryDraft.flow_id = flowSelect.value || null;
      });
    }
  }
"###;

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn script_reads_and_writes_libraries() {
        assert!(SCRIPT.contains("function loadLibraries"));
        assert!(SCRIPT.contains("function saveLibrary"));
        assert!(SCRIPT.contains("function addLibraryWorker"));
        assert!(SCRIPT.contains("/api/v1/libraries"));
        assert!(SCRIPT.contains("read_write"));
        assert!(SCRIPT.contains("libraries-fact"));
    }
}
