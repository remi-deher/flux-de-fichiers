//! Script client pour la synchronisation réactive avec l'API coordinateur.

pub const CLIENT_SCRIPT: &str = r#"
(function() {
  'use strict';

  const STORAGE_KEY = 'flux_admin_key';
  let adminKey = sessionStorage.getItem(STORAGE_KEY) || '';

  const el = (id) => document.getElementById(id);

  function showNotice(text, type = 'info') {
    const notice = el('live-notice');
    if (!notice) return;
    notice.textContent = text;
    notice.className = 'toast-notice visible ' + type;
    setTimeout(() => {
      if (notice.textContent === text) {
        notice.className = 'toast-notice';
      }
    }, 6000);
  }

  function formatBytes(bytes) {
    if (bytes === 0 || !bytes) return '0 o';
    const k = 1024;
    const sizes = ['o', 'Kio', 'Mio', 'Gio', 'Tio'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  function formatDate(isoStr) {
    if (!isoStr) return '—';
    try {
      const d = new Date(isoStr);
      return d.toLocaleString('fr-FR', {
        dateStyle: 'short',
        timeStyle: 'medium'
      });
    } catch {
      return isoStr;
    }
  }

  async function checkHealth() {
    const pill = el('health-status-pill');
    const label = el('health-status-text');
    const metricHealth = el('metric-health');

    try {
      const res = await fetch('/health');
      if (res.status === 204 || res.ok) {
        if (pill) {
          pill.className = 'status-pill online';
          pill.setAttribute('aria-label', 'Coordinateur joignable');
        }
        if (label) label.textContent = 'Coordinateur joignable';
        if (metricHealth) metricHealth.textContent = 'Joignable';
        return true;
      } else {
        throw new Error('Code HTTP ' + res.status);
      }
    } catch (e) {
      if (pill) {
        pill.className = 'status-pill offline';
        pill.setAttribute('aria-label', 'Coordinateur inaccessible');
      }
      if (label) label.textContent = 'Inaccessible';
      if (metricHealth) metricHealth.textContent = 'Inaccessible';
      return false;
    }
  }

  async function apiFetch(endpoint, options = {}) {
    if (!adminKey) {
      throw new Error('Clé administrateur manquante');
    }
    const headers = {
      'Authorization': 'Bearer ' + adminKey,
      'Content-Type': 'application/json',
      ...(options.headers || {})
    };
    const res = await fetch(endpoint, { ...options, headers });
    if (res.status === 401) {
      showNotice('Clé administrateur refusée (401 Non autorisé)', 'error');
      throw new Error('Authentification échouée');
    }
    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      throw new Error(err.error?.code || 'Erreur HTTP ' + res.status);
    }
    return res;
  }

  async function downloadJobOutput(jobId, filename) {
    if (!adminKey) {
      showNotice('Clé administrateur requise pour télécharger le résultat.', 'error');
      return;
    }
    showNotice(`Téléchargement authentifié de la tâche ${jobId.slice(0, 8)}…`, 'info');
    try {
      const res = await fetch(`/api/v1/jobs/${jobId}/output`, {
        headers: {
          'Authorization': 'Bearer ' + adminKey
        }
      });
      if (res.status === 401) {
        showNotice('Téléchargement refusé : clé administrateur invalide ou expirée.', 'error');
        return;
      }
      if (!res.ok) {
        showNotice(`Erreur lors du téléchargement : code HTTP ${res.status}`, 'error');
        return;
      }
      const blob = await res.blob();
      const objectUrl = URL.createObjectURL(blob);
      const tempLink = document.createElement('a');
      tempLink.href = objectUrl;
      tempLink.download = filename || `remux-${jobId.slice(0, 8)}.mkv`;
      document.body.appendChild(tempLink);
      tempLink.click();
      document.body.removeChild(tempLink);
      setTimeout(() => URL.revokeObjectURL(objectUrl), 10000);
      showNotice(`Téléchargement de ${tempLink.download} terminé.`, 'success');
    } catch (e) {
      showNotice(`Échec du téléchargement en mémoire : ${e.message}. Pour les fichiers de plusieurs Go, privilégiez curl en direct.`, 'error');
    }
  }

  async function loadFiles() {
    const tbody = el('files-table-body');
    const countEl = el('metric-files');
    if (!adminKey) {
      if (tbody) tbody.innerHTML = '<tr><td colspan="3" class="empty-state-text">Veuillez renseigner votre clé administrateur ci-dessus pour lister les fichiers sources.</td></tr>';
      if (countEl) countEl.textContent = '—';
      return;
    }

    try {
      const res = await apiFetch('/api/v1/files');
      const files = await res.json();
      if (countEl) countEl.textContent = files.length.toString();

      if (!tbody) return;
      if (files.length === 0) {
        tbody.innerHTML = '<tr><td colspan="3"><div class="empty-state"><p class="empty-state-title">Aucun fichier source disponible</p><p class="empty-state-text">Le répertoire d\'entrée (INPUT_DIR) ne contient actuellement aucun fichier .mkv admissible.</p></div></td></tr>';
        return;
      }

      tbody.innerHTML = files.map(f => `
        <tr>
          <td><strong class="code-cell">${escapeHtml(f.name)}</strong></td>
          <td>${formatBytes(f.bytes)}</td>
          <td>
            <button type="button" class="btn btn-primary btn-sm btn-action-remux" data-filename="${escapeHtml(f.name)}">
              Lancer le remuxage
            </button>
          </td>
        </tr>
      `).join('');

      tbody.querySelectorAll('.btn-action-remux').forEach(btn => {
        btn.addEventListener('click', () => {
          const filename = btn.getAttribute('data-filename');
          if (filename) createJob(filename);
        });
      });
    } catch (e) {
      if (tbody) tbody.innerHTML = `<tr><td colspan="3" class="empty-state-text">Erreur de chargement des fichiers : ${escapeHtml(e.message)}</td></tr>`;
    }
  }

  async function loadJobs() {
    const tbody = el('jobs-table-body');
    const countEl = el('metric-jobs');
    if (!adminKey) {
      if (tbody) tbody.innerHTML = '<tr><td colspan="5" class="empty-state-text">Clé administrateur requise pour afficher les tâches.</td></tr>';
      if (countEl) countEl.textContent = '—';
      return;
    }

    try {
      const res = await apiFetch('/api/v1/jobs');
      const jobs = await res.json();
      if (countEl) countEl.textContent = jobs.length.toString();

      if (!tbody) return;
      if (jobs.length === 0) {
        tbody.innerHTML = '<tr><td colspan="5"><div class="empty-state"><p class="empty-state-title">Aucune tâche enregistrée</p><p class="empty-state-text">Aucun traitement de remuxage n\'a encore été soumis ou historisé dans PostgreSQL.</p></div></td></tr>';
        return;
      }

      tbody.innerHTML = jobs.map(j => {
        const status = j.status || 'inconnu';
        let statusPill = `<span class="status-pill queued"><span class="status-dot"></span>En attente</span>`;
        let downloadAction = '<span class="metric-desc">Non disponible</span>';

        if (status === 'queued') {
          statusPill = `<span class="status-pill queued"><span class="status-dot"></span>En file d'attente</span>`;
        } else if (status === 'running') {
          statusPill = `<span class="status-pill running"><span class="status-dot"></span>En cours</span>`;
        } else if (status === 'succeeded') {
          statusPill = `<span class="status-pill succeeded"><span class="status-dot"></span>Succès</span>`;
          downloadAction = `<button type="button" class="btn btn-secondary btn-sm btn-action-download" data-jobid="${j.id}" data-filename="${escapeHtml(j.output_name || j.source_name)}">Télécharger (API)</button>`;
        } else if (status === 'failed') {
          const errDetail = j.error_code ? ` (${escapeHtml(j.error_code)})` : '';
          statusPill = `<span class="status-pill failed"><span class="status-dot"></span>Échec${errDetail}</span>`;
        }

        return `
          <tr>
            <td class="code-cell">${j.id.slice(0, 8)}…</td>
            <td><strong>${escapeHtml(j.source_name)}</strong></td>
            <td>${statusPill}</td>
            <td>${formatDate(j.created_at)}</td>
            <td>${downloadAction}</td>
          </tr>
        `;
      }).join('');

      tbody.querySelectorAll('.btn-action-download').forEach(btn => {
        btn.addEventListener('click', () => {
          const jobId = btn.getAttribute('data-jobid');
          const filename = btn.getAttribute('data-filename');
          if (jobId) downloadJobOutput(jobId, filename);
        });
      });
    } catch (e) {
      if (tbody) tbody.innerHTML = `<tr><td colspan="5" class="empty-state-text">Erreur de chargement des tâches : ${escapeHtml(e.message)}</td></tr>`;
    }
  }

  async function loadWorkers() {
    const tbody = el('workers-table-body');
    const countEl = el('metric-workers');
    if (!adminKey) {
      if (tbody) tbody.innerHTML = '<tr><td colspan="4" class="empty-state-text">Clé administrateur requise pour lister les agents.</td></tr>';
      if (countEl) countEl.textContent = '—';
      return;
    }

    try {
      const res = await apiFetch('/api/v1/workers');
      const workers = await res.json();
      if (countEl) countEl.textContent = workers.length.toString();

      if (!tbody) return;
      if (workers.length === 0) {
        tbody.innerHTML = '<tr><td colspan="4"><div class="empty-state"><p class="empty-state-title">Aucun agent enregistré</p><p class="empty-state-text">Aucun worker ne s\'est encore enregistré auprès du coordinateur.</p></div></td></tr>';
        return;
      }

      tbody.innerHTML = workers.map(w => `
        <tr>
          <td class="code-cell">${w.id.slice(0, 8)}…</td>
          <td><strong>${escapeHtml(w.name)}</strong></td>
          <td><span class="status-pill online"><span class="status-dot"></span>Enregistré</span></td>
          <td>${formatDate(w.last_seen)}</td>
        </tr>
      `).join('');
    } catch (e) {
      if (tbody) tbody.innerHTML = `<tr><td colspan="4" class="empty-state-text">Erreur de chargement des agents : ${escapeHtml(e.message)}</td></tr>`;
    }
  }

  async function createJob(sourceName) {
    if (!adminKey) {
      showNotice('Veuillez d\'abord saisir votre clé administrateur ci-dessus.', 'error');
      return;
    }
    showNotice(`Création de la tâche pour "${sourceName}"…`, 'info');
    try {
      const res = await apiFetch('/api/v1/jobs', {
        method: 'POST',
        body: JSON.stringify({ source_name: sourceName, target_worker_id: null })
      });
      const job = await res.json();
      showNotice(`Tâche ${job.id.slice(0, 8)} créée avec succès pour ${sourceName}`, 'success');
      loadJobs();
    } catch (e) {
      showNotice(`Échec de création de la tâche : ${e.message}`, 'error');
    }
  }

  function escapeHtml(str) {
    if (!str) return '';
    return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  function setupAuthForm() {
    const keyInput = el('admin-key-input');
    const saveBtn = el('admin-key-save-btn');
    const statusText = el('auth-status-text');

    if (keyInput && adminKey) {
      keyInput.value = adminKey;
      if (statusText) statusText.textContent = 'Clé enregistrée en session locale (navigateur uniquement).';
    }

    if (saveBtn && keyInput) {
      saveBtn.addEventListener('click', () => {
        adminKey = keyInput.value.trim();
        sessionStorage.setItem(STORAGE_KEY, adminKey);
        if (statusText) {
          statusText.textContent = adminKey ? 'Clé enregistrée dans la session locale.' : 'Clé effacée.';
        }
        showNotice('Configuration de la clé mise à jour.', 'info');
        loadAll();
      });
    }
  }

  function setupRefresh() {
    const refreshBtn = el('refresh-all-btn');
    if (refreshBtn) {
      refreshBtn.addEventListener('click', () => {
        showNotice('Actualisation des données…', 'info');
        loadAll();
      });
    }
  }

  function loadAll() {
    checkHealth();
    loadFiles();
    loadJobs();
    loadWorkers();
  }

  document.addEventListener('DOMContentLoaded', () => {
    setupAuthForm();
    setupRefresh();
    loadAll();
    // Interrogation périodique de santé toutes les 30 secondes
    setInterval(checkHealth, 30000);
  });
})();
"#;
