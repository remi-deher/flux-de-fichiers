//! Exploration des médias et dialogue de remuxage.

pub const SCRIPT: &str = r###"
// --- Fichiers sources ---
  async function loadFiles() {
    const tbody = el('files-table-body');
    const metricCount = el('metric-files-count');

    try {
      const res = await apiFetch('/api/v1/files');
      const data = await res.json();
      cachedFiles = Array.isArray(data) ? data : (data.files || []);

      if (metricCount) metricCount.textContent = String(cachedFiles.length);
      renderFilesTable();
    } catch (e) {
      if (tbody) {
        tbody.innerHTML = `<tr><td colspan="3" class="empty-state-text" style="color: var(--danger-text);">
          ${escapeHtml(e.message === 'Non autorisé' ? 'Veuillez ouvrir une session dans l\'onglet Paramètres pour lister les fichiers.' : 'Impossible de récupérer les fichiers sources.')}
        </td></tr>`;
      }
    }
  }

  function renderFilesTable() {
    const tbody = el('files-table-body');
    const countBadge = el('files-counter-text');
    const searchInput = el('files-search-input');
    const term = (searchInput ? searchInput.value : '').toLowerCase().trim();

    if (!tbody) return;

    const filtered = cachedFiles.filter(f => !term || f.name.toLowerCase().includes(term));

    if (countBadge) {
      countBadge.textContent = `${filtered.length} / ${cachedFiles.length} fichier(s)`;
    }

    if (filtered.length === 0) {
      const msg = cachedFiles.length === 0
        ? 'Aucun fichier source .mkv détecté dans var/input/. Déposez des fichiers pour commencer.'
        : 'Aucun fichier ne correspond à votre recherche.';
      tbody.innerHTML = `<tr><td colspan="3" class="empty-state-text">${escapeHtml(msg)}</td></tr>`;
      return;
    }

    tbody.innerHTML = filtered.map(file => {
      const safeName = escapeHtml(file.name);
      return `
        <tr>
          <td>
            <div style="display: flex; align-items: center; gap: 0.5rem; font-weight: 600;">
              <span aria-hidden="true" style="color: var(--primary);">🎬</span>
              <span>${safeName}</span>
            </div>
          </td>
          <td style="font-family: var(--font-mono);">${formatBytes(file.bytes)}</td>
          <td style="text-align: right;">
            <button
              type="button"
              class="btn btn-primary btn-sm remux-launch-btn"
              data-filename="${safeName}"
              aria-label="Lancer le remuxage du fichier ${safeName}"
            >
              Lancer le remuxage
            </button>
          </td>
        </tr>
      `;
    }).join('');

    tbody.querySelectorAll('.remux-launch-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        openConfirmRemuxDialog(btn.getAttribute('data-filename'));
      });
    });
  }

  // --- Modale de confirmation de remuxage ---
  function openConfirmRemuxDialog(filename) {
    selectedFileForRemux = filename;
    const dialog = el('confirm-remux-dialog');
    const filenameEl = el('confirm-dialog-filename');
    const workerSelect = el('confirm-dialog-worker-select');

    if (!dialog) return;

    if (filenameEl) filenameEl.textContent = filename;

    // Alimenter la sélection des agents
    if (workerSelect) {
      workerSelect.innerHTML = '<option value="">N\'importe quel agent disponible (recommandé)</option>';
      cachedWorkers.forEach(w => {
        const opt = document.createElement('option');
        opt.value = w.id;
        opt.textContent = `${w.name} (${w.id.slice(0, 8)})`;
        workerSelect.appendChild(opt);
      });
    }

    dialog.showModal();
    const submitBtn = el('confirm-dialog-submit-btn');
    if (submitBtn) submitBtn.focus();
  }

  function closeConfirmRemuxDialog() {
    const dialog = el('confirm-remux-dialog');
    if (dialog && dialog.open) {
      dialog.close();
    }
    selectedFileForRemux = null;
  }

  async function submitRemuxJob() {
    if (!selectedFileForRemux) return;
    const workerSelect = el('confirm-dialog-worker-select');
    const targetWorker = workerSelect && workerSelect.value ? workerSelect.value : null;

    closeConfirmRemuxDialog();
    showNotice(`Soumission du remuxage pour ${selectedFileForRemux}…`, 'info');

    try {
      const res = await apiFetch('/api/v1/jobs', {
        method: 'POST',
        body: JSON.stringify({
          source_name: selectedFileForRemux,
          target_worker_id: targetWorker
        })
      });
      const job = await res.json();
      showNotice(`Tâche créée avec succès (ID: ${job.id.slice(0, 8)}).`, 'success');
      switchTab('jobs');
      refreshAll();
    } catch (e) {
      showNotice('Échec de la soumission de la tâche : ' + e.message, 'error');
    }
  }

"###;
