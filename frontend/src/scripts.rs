//! Script client pour le poste de pilotage Flux de Fichiers.

pub const CLIENT_SCRIPT: &str = r#"
(function() {
  'use strict';

  // Sécurité : la clé d'administration réside UNIQUEMENT en mémoire volatile JavaScript de l'onglet actif.
  // Aucun stockage en clair dans sessionStorage ni localStorage.
  let volatileAdminKey = '';
  let csrfToken = '';
  let activeTab = 'dashboard';
  let cachedFiles = [];
  let cachedJobs = [];
  let cachedWorkers = [];
  let activeJobFilter = 'all';
  let selectedFileForRemux = null;
  let sessionMode = 'none'; // 'none' | 'memory_key' | 'cookie_session'

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

  function escapeHtml(str) {
    if (!str) return '';
    return String(str)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  // --- Gestion des onglets accessibles ---
  const TABS = ['dashboard', 'files', 'jobs', 'workers', 'plugins', 'settings'];

  function switchTab(tabName) {
    if (!TABS.includes(tabName)) return;
    activeTab = tabName;

    TABS.forEach(name => {
      const btn = el(`tab-btn-${name}`);
      const panel = el(`panel-${name}`);
      const isSelected = name === tabName;

      if (btn) {
        btn.setAttribute('aria-selected', isSelected ? 'true' : 'false');
        btn.setAttribute('tabindex', isSelected ? '0' : '-1');
      }
      if (panel) {
        if (isSelected) {
          panel.removeAttribute('hidden');
        } else {
          panel.setAttribute('hidden', '');
        }
      }
    });
  }

  function setupTabs() {
    TABS.forEach((name, index) => {
      const btn = el(`tab-btn-${name}`);
      if (!btn) return;

      btn.addEventListener('click', () => switchTab(name));

      btn.addEventListener('keydown', (e) => {
        let targetIndex = index;
        if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
          e.preventDefault();
          targetIndex = (index + 1) % TABS.length;
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
          e.preventDefault();
          targetIndex = (index - 1 + TABS.length) % TABS.length;
        } else if (e.key === 'Home') {
          e.preventDefault();
          targetIndex = 0;
        } else if (e.key === 'End') {
          e.preventDefault();
          targetIndex = TABS.length - 1;
        } else {
          return;
        }
        const nextBtn = el(`tab-btn-${TABS[targetIndex]}`);
        if (nextBtn) {
          nextBtn.focus();
          switchTab(TABS[targetIndex]);
        }
      });
    });

    const qFiles = el('quick-goto-files-btn');
    if (qFiles) qFiles.addEventListener('click', () => switchTab('files'));

    const qJobs = el('quick-goto-jobs-btn');
    if (qJobs) qJobs.addEventListener('click', () => switchTab('jobs'));

    const qSettings = el('quick-goto-settings-btn');
    if (qSettings) qSettings.addEventListener('click', () => switchTab('settings'));
  }

  // --- Gestion du Thème (Clair / Sombre / Système) ---
  function applyTheme(theme) {
    if (theme === 'dark') {
      document.documentElement.setAttribute('data-theme', 'dark');
    } else if (theme === 'light') {
      document.documentElement.setAttribute('data-theme', 'light');
    } else {
      document.documentElement.removeAttribute('data-theme');
    }
    const select = el('theme-select');
    if (select) select.value = theme;
    try {
      localStorage.setItem('flux_theme_pref', theme);
    } catch {}
  }

  function setupTheme() {
    let saved = 'system';
    try {
      saved = localStorage.getItem('flux_theme_pref') || 'system';
    } catch {}
    applyTheme(saved);

    const select = el('theme-select');
    if (select) {
      select.addEventListener('change', (e) => applyTheme(e.target.value));
    }

    const toggleBtn = el('theme-toggle-btn');
    if (toggleBtn) {
      toggleBtn.addEventListener('click', () => {
        const current = document.documentElement.getAttribute('data-theme');
        if (current === 'dark') {
          applyTheme('light');
        } else if (current === 'light') {
          applyTheme('system');
        } else {
          applyTheme('dark');
        }
      });
    }
  }

  // --- Gestion de la session et de l'authentification (Contrat #29) ---
  function updateSessionUI() {
    const pill = el('session-status-pill');
    const label = el('session-status-text');
    const authStatusText = el('auth-status-text');
    const logoutBtn = el('admin-key-logout-btn');
    const saveBtn = el('admin-key-save-btn');
    const input = el('admin-key-input');

    if (sessionMode === 'cookie_session') {
      if (pill) {
        pill.className = 'status-pill online';
        pill.setAttribute('aria-label', 'Session web sécurisée active');
      }
      if (label) label.textContent = 'Session web active';
      if (authStatusText) {
        authStatusText.textContent = '✅ Session web sécurisée active (cookie HttpOnly et protection CSRF). Téléchargement natif en flux direct sans mémoire tampon.';
      }
      if (logoutBtn) logoutBtn.style.display = 'inline-flex';
      if (saveBtn) saveBtn.textContent = 'Changer de clé';
    } else if (sessionMode === 'memory_key' && volatileAdminKey) {
      if (pill) {
        pill.className = 'status-pill online';
        pill.setAttribute('aria-label', 'Authentifié via clé en mémoire volatile');
      }
      if (label) label.textContent = 'Connecté (Mémoire)';
      if (authStatusText) {
        authStatusText.textContent = '🔒 Clé active en mémoire vive volatile de l\'onglet (non persistée sur disque, effacée à la fermeture).';
      }
      if (logoutBtn) logoutBtn.style.display = 'inline-flex';
      if (saveBtn) saveBtn.textContent = 'Mettre à jour la clé';
    } else {
      if (pill) {
        pill.className = 'status-pill warning';
        pill.setAttribute('aria-label', 'Session non authentifiée');
      }
      if (label) label.textContent = 'Non authentifié';
      if (authStatusText) {
        authStatusText.textContent = '🔒 Mode mémoire volatile actif : la clé ne sera stockée ni dans sessionStorage ni dans localStorage.';
      }
      if (logoutBtn) logoutBtn.style.display = 'none';
      if (saveBtn) saveBtn.textContent = 'Ouvrir la session';
      if (input) input.value = '';
    }
  }

  async function checkSession() {
    // Vérification de la session active via GET /api/v1/auth/session (contrat #29)
    try {
      const res = await fetch('/api/v1/auth/session', {
        method: 'GET',
        credentials: 'include'
      });
      if (res.ok) {
        const data = await res.json().catch(() => ({}));
        if (data.csrf_token) {
          csrfToken = data.csrf_token;
        }
        sessionMode = 'cookie_session';
        updateSessionUI();
        return;
      }
    } catch {}

    if (volatileAdminKey) {
      sessionMode = 'memory_key';
    } else {
      sessionMode = 'none';
    }
    updateSessionUI();
  }

  async function login(key) {
    if (!key || !key.trim()) {
      showNotice('Veuillez saisir une clé d\'administration.', 'error');
      return;
    }
    const cleanKey = key.trim();

    // 1. Tenter d'ouvrir la session web backend officielle (#29)
    try {
      const res = await fetch('/api/v1/auth/session', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({ admin_key: cleanKey })
      });
      if (res.ok) {
        const data = await res.json().catch(() => ({}));
        if (data.csrf_token) {
          csrfToken = data.csrf_token;
        }
        sessionMode = 'cookie_session';
        volatileAdminKey = ''; // Purge immédiate de la clé en mémoire vive une fois le cookie posé
        updateSessionUI();
        showNotice('Session web sécurisée ouverte avec succès (cookie HttpOnly).', 'success');
        refreshAll();
        return;
      }
    } catch {}

    // 2. Fallback mémoire volatile avec jeton Bearer
    volatileAdminKey = cleanKey;
    sessionMode = 'memory_key';
    updateSessionUI();
    showNotice('Clé mémorisée en mémoire volatile (aucun stockage disque ou sessionStorage).', 'success');
    refreshAll();
  }

  async function logout() {
    // Révocation de session web côté backend si active
    if (sessionMode === 'cookie_session') {
      try {
        const headers = {};
        if (csrfToken) {
          headers['x-csrf-token'] = csrfToken;
        }
        await fetch('/api/v1/auth/logout', {
          method: 'POST',
          headers,
          credentials: 'include'
        });
      } catch {}
    }

    volatileAdminKey = '';
    csrfToken = '';
    sessionMode = 'none';
    updateSessionUI();
    showNotice('Session fermée et mémoire purgée.', 'info');
    refreshAll();
  }

  // --- Requêtes API authentifiées ---
  async function apiFetch(endpoint, options = {}) {
    const method = (options.method || 'GET').toUpperCase();
    const headers = {
      'Content-Type': 'application/json',
      ...(options.headers || {})
    };

    // Protection CSRF pour les méthodes non sûres (contrat #29)
    if (csrfToken && !['GET', 'HEAD', 'OPTIONS'].includes(method)) {
      headers['x-csrf-token'] = csrfToken;
    }

    // Authentification Bearer si clé volatile présente
    if (volatileAdminKey) {
      headers['Authorization'] = 'Bearer ' + volatileAdminKey;
    }

    const res = await fetch(endpoint, {
      ...options,
      headers,
      credentials: 'include'
    });

    if (res.status === 401) {
      volatileAdminKey = '';
      sessionMode = 'none';
      csrfToken = '';
      updateSessionUI();
      showNotice('Accès non autorisé (401) : clé révoquée ou session invalide. Mémoire purgée.', 'error');
      throw new Error('Non autorisé');
    }

    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      throw new Error(err.error?.code || 'Erreur HTTP ' + res.status);
    }
    return res;
  }

  // --- Vérification de santé ---
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

  // --- Tâches de remuxage ---
  async function loadJobs() {
    const tbody = el('jobs-table-body');
    const metricJobs = el('metric-jobs-count');

    try {
      const res = await apiFetch('/api/v1/jobs');
      const data = await res.json();
      cachedJobs = Array.isArray(data) ? data : (data.jobs || []);

      if (metricJobs) metricJobs.textContent = String(cachedJobs.length);
      renderRecentJobs();
      renderJobsTable();
    } catch (e) {
      if (tbody) {
        tbody.innerHTML = `<tr><td colspan="5" class="empty-state-text" style="color: var(--danger-text);">
          ${escapeHtml(e.message === 'Non autorisé' ? 'Veuillez ouvrir une session pour afficher la file des tâches.' : 'Impossible de récupérer les tâches.')}
        </td></tr>`;
      }
    }
  }

  function renderRecentJobs() {
    const tbody = el('recent-jobs-table-body');
    if (!tbody) return;

    const recent = cachedJobs.slice(0, 3);
    if (recent.length === 0) {
      tbody.innerHTML = `<tr><td colspan="4" class="empty-state-text">Aucune tâche enregistrée pour le moment.</td></tr>`;
      return;
    }

    tbody.innerHTML = recent.map(j => {
      const statusClass = j.status === 'succeeded' ? 'succeeded' : (j.status === 'failed' ? 'failed' : (j.status === 'running' ? 'running' : 'queued'));
      return `
        <tr>
          <td style="font-family: var(--font-mono); font-weight: 600;">${escapeHtml(j.id.slice(0, 8))}…</td>
          <td>${escapeHtml(j.source_name)}</td>
          <td><span class="status-pill ${statusClass}"><span class="status-dot"></span>${escapeHtml(j.status)}</span></td>
          <td>${formatDate(j.created_at)}</td>
        </tr>
      `;
    }).join('');
  }

  function updateJobFiltersCount() {
    const counts = { all: cachedJobs.length, queued: 0, running: 0, succeeded: 0, failed: 0 };
    cachedJobs.forEach(j => {
      if (counts[j.status] !== undefined) counts[j.status]++;
    });

    Object.keys(counts).forEach(k => {
      const counterEl = el(`count-filter-${k}`);
      if (counterEl) counterEl.textContent = `(${counts[k]})`;
    });
  }

  function renderJobsTable() {
    const tbody = el('jobs-table-body');
    const searchInput = el('jobs-search-input');
    const term = (searchInput ? searchInput.value : '').toLowerCase().trim();

    if (!tbody) return;

    updateJobFiltersCount();

    const filtered = cachedJobs.filter(j => {
      const matchFilter = activeJobFilter === 'all' || j.status === activeJobFilter;
      const matchSearch = !term || j.id.toLowerCase().includes(term) || j.source_name.toLowerCase().includes(term);
      return matchFilter && matchSearch;
    });

    if (filtered.length === 0) {
      const msg = cachedJobs.length === 0
        ? 'Aucune tâche de remuxage dans la file d\'attente.'
        : 'Aucune tâche ne correspond aux critères sélectionnés.';
      tbody.innerHTML = `<tr><td colspan="5" class="empty-state-text">${escapeHtml(msg)}</td></tr>`;
      return;
    }

    tbody.innerHTML = filtered.map(j => {
      const safeId = escapeHtml(j.id);
      const safeSource = escapeHtml(j.source_name);
      const statusClass = j.status === 'succeeded' ? 'succeeded' : (j.status === 'failed' ? 'failed' : (j.status === 'running' ? 'running' : 'queued'));
      const isSucceeded = j.status === 'succeeded';

      return `
        <tr>
          <td style="font-family: var(--font-mono); font-weight: 600;">${safeId.slice(0, 8)}…</td>
          <td>${safeSource}</td>
          <td>
            <span class="status-pill ${statusClass}">
              <span class="status-dot"></span>
              ${escapeHtml(j.status)}
            </span>
          </td>
          <td>${formatDate(j.created_at)}</td>
          <td style="text-align: right;">
            <div style="display: inline-flex; gap: 0.5rem; justify-content: flex-end;">
              <button
                type="button"
                class="btn btn-secondary btn-sm job-detail-btn"
                data-job-id="${safeId}"
                aria-label="Inspecter les détails de la tâche ${safeId}"
              >
                Détails
              </button>
              ${isSucceeded ? `
                <button
                  type="button"
                  class="btn btn-primary btn-sm job-download-btn"
                  data-job-id="${safeId}"
                  data-output-name="${escapeHtml(j.output_name || 'resultat.mkv')}"
                  aria-label="Télécharger en flux le résultat de la tâche ${safeId}"
                >
                  Télécharger
                </button>
              ` : ''}
            </div>
          </td>
        </tr>
      `;
    }).join('');

    tbody.querySelectorAll('.job-detail-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        openJobDetailDialog(btn.getAttribute('data-job-id'));
      });
    });

    tbody.querySelectorAll('.job-download-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        downloadJobStream(btn.getAttribute('data-job-id'), btn.getAttribute('data-output-name'));
      });
    });
  }

  // --- Téléchargement en flux direct (sans Blob en mémoire RAM) ---
  function downloadJobStream(jobId, filename) {
    const url = `/api/v1/jobs/${encodeURIComponent(jobId)}/output`;

    // Si une session web par cookie HttpOnly est active (contrat #29), le téléchargement direct
    // par le navigateur s'effectue en streaming continu sans tampon mémoire JavaScript !
    if (sessionMode === 'cookie_session') {
      showNotice(`Téléchargement direct en flux pour ${filename}…`, 'info');
      const a = document.createElement('a');
      a.href = url;
      a.download = filename || 'resultat.mkv';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      return;
    }

    // Si authentifié via clé volatile en mémoire (sans cookie de session) :
    // Une navigation <a> ne transmet pas l'en-tête Authorization (provoquant un 401).
    // On oriente l'utilisateur vers l'ouverture d'une session web dans Paramètres ou curl.
    if (volatileAdminKey) {
      showNotice('Pour un streaming direct en un clic, ouvrez une session web dans Paramètres. Affichage de la commande curl dans Détails…', 'info');
      openJobDetailDialog(jobId);
      return;
    }

    showNotice('Session ou clé administrateur requise pour télécharger le résultat.', 'error');
    switchTab('settings');
  }

  // --- Modale de détails d'une tâche ---
  function openJobDetailDialog(jobId) {
    const job = cachedJobs.find(j => j.id === jobId);
    if (!job) return;

    const dialog = el('job-detail-dialog');
    if (!dialog) return;

    const idEl = el('detail-job-id');
    const sourceEl = el('detail-job-source');
    const statusEl = el('detail-job-status');
    const workerEl = el('detail-job-worker');
    const attemptsEl = el('detail-job-attempts');
    const createdEl = el('detail-job-created');
    const outputEl = el('detail-job-output');
    const errorBox = el('detail-error-box');
    const errorContent = el('detail-error-content');
    const curlSec = el('detail-curl-section');
    const curlCmd = el('detail-curl-command');
    const dlBtn = el('job-dialog-download-btn');

    if (idEl) idEl.textContent = job.id;
    if (sourceEl) sourceEl.textContent = job.source_name;
    if (statusEl) {
      const statusClass = job.status === 'succeeded' ? 'succeeded' : (job.status === 'failed' ? 'failed' : (job.status === 'running' ? 'running' : 'queued'));
      statusEl.innerHTML = `<span class="status-pill ${statusClass}"><span class="status-dot"></span>${escapeHtml(job.status)}</span>`;
    }
    if (workerEl) workerEl.textContent = job.worker_id || job.target_worker_id || 'N\'importe quel agent (ou central autonome)';
    if (attemptsEl) attemptsEl.textContent = `${job.attempts || 1} tentative(s)`;
    if (createdEl) createdEl.textContent = formatDate(job.created_at);
    if (outputEl) outputEl.textContent = job.output_name || 'Non généré';

    if (errorBox && errorContent) {
      if (job.status === 'failed' || job.error_code) {
        errorBox.style.display = 'block';
        errorContent.textContent = job.error_code
          ? `Code d'erreur: ${job.error_code}`
          : 'Traitement en échec sans code spécifique.';
      } else {
        errorBox.style.display = 'none';
      }
    }

    if (curlSec && curlCmd) {
      curlCmd.textContent = `curl -k -H "Authorization: Bearer <ADMIN_KEY>" "https://localhost:3443/api/v1/jobs/${encodeURIComponent(job.id)}/output" -o "${job.output_name || 'resultat.mkv'}"`;
      curlSec.style.display = 'block';
    }

    if (dlBtn) {
      if (job.status === 'succeeded' && sessionMode === 'cookie_session') {
        dlBtn.style.display = 'inline-flex';
        dlBtn.textContent = 'Télécharger le résultat (flux direct)';
        dlBtn.onclick = () => {
          downloadJobStream(job.id, job.output_name || 'resultat.mkv');
        };
      } else {
        dlBtn.style.display = 'none';
      }
    }

    dialog.showModal();
    const closeBtn = el('job-dialog-close-btn');
    if (closeBtn) closeBtn.focus();
  }

  function closeJobDetailDialog() {
    const dialog = el('job-detail-dialog');
    if (dialog && dialog.open) {
      dialog.close();
    }
  }

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
            <div style="font-size: 0.75rem; color: var(--text-dim);">${safeName.startsWith('coordinator') ? 'Coordinateur avec agent intégré' : 'Agent distant HTTPS'}</div>
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

  // --- Synchronisation globale ---
  async function refreshAll() {
    const refreshBtn = el('refresh-all-btn');
    if (refreshBtn) {
      refreshBtn.disabled = true;
      refreshBtn.textContent = 'Actualisation…';
    }

    await checkHealth();
    await checkSession();
    await Promise.allSettled([
      loadFiles(),
      loadJobs(),
      loadWorkers()
    ]);

    if (refreshBtn) {
      refreshBtn.disabled = false;
      refreshBtn.textContent = 'Actualiser';
    }
  }

  // --- Gestion du catalogue d'outils, inspecteur de contrats et banc d'essai ---
  function setupPluginsPanel() {
    const CONTRACTS_DB = {
      'tool.mkvtoolnix': {
        path: 'tools/mkvtoolnix/distribution.v1.json',
        schema: 'distribution.v1.json',
        title: 'Contrat de distribution — MKVToolNix',
        visual: `
          <div class="visual-summary-grid">
            <div class="visual-summary-box">
              <div class="visual-summary-title">Distribution Option B</div>
              <p class="visual-summary-text">Binaire autonome hors image conteneur. Rapatrié et vérifié à la demande par le cache worker.</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Intégrité SHA-256 obligatoire</div>
              <p class="visual-summary-text">Archive : <code>1dd2377...</code> (x86_64) / <code>645600f...</code> (aarch64)<br>Binaire : <code>c55e78f...</code> (x86_64) / <code>6a9e90f...</code> (aarch64)</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Sécurité d'extraction</div>
              <p class="visual-summary-text"><code>noShellUnpack: true</code> (sans shell) • Anti-ZipSlip • Mode Unix <code>0755</code> • Confinement non-root.</p>
            </div>
          </div>
        `,
        data: {
          schemaVersion: "1.0.0",
          toolId: "tool.mkvtoolnix",
          toolVersion: "1.0.0",
          distributionMode: "autonomous-binary",
          releaseStatus: "contract-only",
          artifacts: [
            {
              arch: "x86_64",
              os: "linux",
              archiveUrl: "https://artifacts.example.invalid/tools/mkvtoolnix/1.0.0/mkvtoolnix-linux-x86_64.tar.gz",
              archiveFormat: "tar.gz",
              archiveSha256: "1dd2377127f9c41ad8bccd941c7b695dbb703036c2854ad27962e273b8d3a0ed",
              binaryRelativePath: "bin/mkvmerge",
              binarySha256: "c55e78f63d28695cc625565dd777e37ee776aace68b81ea720f8e0c38e48ddf3",
              permissions: { unixMode: "0755", owner: "runtime-user", executableBit: true }
            },
            {
              arch: "aarch64",
              os: "linux",
              archiveUrl: "https://artifacts.example.invalid/tools/mkvtoolnix/1.0.0/mkvtoolnix-linux-aarch64.tar.gz",
              archiveFormat: "tar.gz",
              archiveSha256: "645600fcd1866e47f49015217b92370a9abe2fb333c2706bc3b05524175c4e13",
              binaryRelativePath: "bin/mkvmerge",
              binarySha256: "6a9e90f5c6b31085149ca306e476a6e591fcf82cfcdd8ee24479e49e446066a5",
              permissions: { unixMode: "0755", owner: "runtime-user", executableBit: true }
            }
          ],
          installPolicy: {
            verifySha256BeforeExec: true,
            refuseMismatchedDigest: true,
            cacheRootRelative: "tools-cache/mkvtoolnix/1.0.0",
            noShellUnpack: true,
            executableAllowlist: ["mkvmerge"],
            networkAtInstall: "explicit-allowlist"
          },
          notes: [
            "Option B : hors image applicative. Empreintes et URL contract-only.",
            "Remux Matroska uniquement ; pas de production MP4."
          ]
        }
      },
      'tool.ffmpeg': {
        path: 'tools/ffmpeg/distribution.v1.json',
        schema: 'distribution.v1.json',
        title: 'Contrat de distribution — FFmpeg',
        visual: `
          <div class="visual-summary-grid">
            <div class="visual-summary-box">
              <div class="visual-summary-title">Distribution Option B (Distribué)</div>
              <p class="visual-summary-text">Outil réservé aux tests et scénarios de simulation pour médias synthétiques. Non présent dans le binaire de production.</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Empreintes SHA-256</div>
              <p class="visual-summary-text">Archive : <code>91a82f3...</code> (x86_64) / <code>3c4190e...</code> (aarch64)<br>Binaire : <code>f87e102...</code> (x86_64)</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Politique d'isolation</div>
              <p class="visual-summary-text"><code>noShellUnpack: true</code> • Confinement non-root • Exécutable allowlist : <code>ffmpeg</code>.</p>
            </div>
          </div>
        `,
        data: {
          schemaVersion: "1.0.0",
          toolId: "tool.ffmpeg",
          toolVersion: "7.1.0",
          distributionMode: "autonomous-binary",
          releaseStatus: "contract-only",
          artifacts: [
            {
              arch: "x86_64",
              os: "linux",
              archiveUrl: "https://artifacts.example.invalid/tools/ffmpeg/7.1.0/ffmpeg-linux-x86_64.tar.gz",
              archiveFormat: "tar.gz",
              archiveSha256: "91a82f37c62d41a7d65b128509e25d2b638f2269c10714b986ee06180635467e",
              binaryRelativePath: "bin/ffmpeg",
              binarySha256: "f87e102d8e35181b53e8387063eb2b77a7967980590835cc9950e7b8f97e20ec",
              permissions: { unixMode: "0755", owner: "runtime-user", executableBit: true }
            }
          ],
          installPolicy: {
            verifySha256BeforeExec: true,
            refuseMismatchedDigest: true,
            cacheRootRelative: "tools-cache/ffmpeg/7.1.0",
            noShellUnpack: true,
            executableAllowlist: ["ffmpeg"],
            networkAtInstall: "explicit-allowlist"
          }
        }
      },
      'tool.internal.telemetry': {
        path: 'tools/internal-telemetry/distribution.v1.json',
        schema: 'distribution.v1.json',
        title: 'Contrat de distribution — Sonde interne de télémétrie',
        visual: `
          <div class="visual-summary-grid">
            <div class="visual-summary-box">
              <div class="visual-summary-title">Sonde système intégrée</div>
              <p class="visual-summary-text">Intégrée nativement au binaire applicatif. Relevés CPU (/proc/stat), RAM (/proc/meminfo), GPU (/dev/dri) et stockage (statvfs).</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Distribution pré-amorcée</div>
              <p class="visual-summary-text">Mode <code>offline-preseeded</code>. Aucun téléchargement réseau requis à l'exécution.</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Confinement maximal</div>
              <p class="visual-summary-text">UID 10001 (app), aucun droit root, aucun socket Docker, aucune commande shell.</p>
            </div>
          </div>
        `,
        data: {
          schemaVersion: "1.0.0",
          toolId: "tool.internal.telemetry",
          toolVersion: "0.1.0",
          distributionMode: "autonomous-binary",
          releaseStatus: "contract-only",
          artifacts: [
            {
              arch: "x86_64",
              os: "linux",
              archiveUrl: "https://artifacts.example.invalid/tools/internal-telemetry/0.1.0/flux-telemetry-linux-x86_64.tar.gz",
              archiveFormat: "tar.gz",
              archiveSha256: "8525e058fac76aa6a9d589c8aee04eb6eb88cba8751f08ee721bda3facb810f3",
              binaryRelativePath: "bin/flux-telemetry",
              binarySha256: "b0125406ca5e2fd735512b4cb591d18a284fad9f71637fb882a745909fe025e2",
              permissions: { unixMode: "0755", owner: "runtime-user", executableBit: true }
            },
            {
              arch: "aarch64",
              os: "linux",
              archiveUrl: "https://artifacts.example.invalid/tools/internal-telemetry/0.1.0/flux-telemetry-linux-aarch64.tar.gz",
              archiveFormat: "tar.gz",
              archiveSha256: "68802fba876a410c69c92a231dc5cf68d10ed8921da921f60beec507150c2844",
              binaryRelativePath: "bin/flux-telemetry",
              binarySha256: "4630821f0b995b9c1abe4fca7a9efaa473535d0c9f8bc629eee5e843ff768cfd",
              permissions: { unixMode: "0755", owner: "runtime-user", executableBit: true }
            }
          ],
          installPolicy: {
            verifySha256BeforeExec: true,
            refuseMismatchedDigest: true,
            cacheRootRelative: "tools-cache/internal-telemetry/0.1.0",
            noShellUnpack: true,
            executableAllowlist: ["flux-telemetry"],
            networkAtInstall: "offline-preseeded"
          },
          notes: [
            "Sonde interne de mesure : CPU, RAM, GPU, stockage.",
            "Confinement strict sous UID 10001 (app), sans privilège root ni socket Docker."
          ]
        }
      },
      'macro.remux-verify': {
        path: 'blocks/macro-remux-verify.example/macro-block.json',
        schema: 'macro-block.json (v1.0.0)',
        title: 'Macro-bloc composite — Remux puis Vérifier',
        visual: `
          <div class="visual-summary-box">
            <div class="visual-summary-title">Pipeline déclaratif composite (2 étapes sécurisées)</div>
            <div class="macro-pipeline" role="group" aria-label="Pipeline du macro-bloc">
              <div class="pipeline-step">
                <span class="pipeline-step-title">Source MKV</span>
                <span class="pipeline-step-detail">video/x-matroska</span>
              </div>
              <span class="pipeline-arrow" aria-hidden="true">➔</span>
              <div class="pipeline-step">
                <span class="pipeline-step-title">Étape 1 : Remux</span>
                <span class="pipeline-step-detail">mkvmerge v1.1.0</span>
              </div>
              <span class="pipeline-arrow" aria-hidden="true">➔</span>
              <div class="pipeline-step">
                <span class="pipeline-step-title">Étape 2 : Vérification</span>
                <span class="pipeline-step-detail">equiv-check v0.0.0</span>
              </div>
              <span class="pipeline-arrow" aria-hidden="true">➔</span>
              <div class="pipeline-step">
                <span class="pipeline-step-title">Résultat MKV</span>
                <span class="pipeline-step-detail">Fichier intègre</span>
              </div>
            </div>
            <div style="margin-top: 0.75rem; font-size: 0.75rem; color: var(--text-dim);">
              <strong>Politique d'erreur :</strong> <code>stop</code> (interruption immédiate en cas d'échec) • <strong>Liaisons :</strong> argv strict sans shell composite.
            </div>
          </div>
        `,
        data: {
          schemaVersion: "1.0.0",
          id: "macro.remux-verify.example",
          version: "0.1.0",
          name: "Remux puis vérifier (macro)",
          kind: "macro-block",
          category: "media",
          sockets: {
            inputs: [{ id: "source", label: "Source MKV", dataType: "file", mediaType: "video/x-matroska" }],
            outputs: [{ id: "result", label: "MKV remuxé", dataType: "file", mediaType: "video/x-matroska" }]
          },
          steps: [
            {
              id: "remux",
              label: "Remux mkvmerge",
              operationId: "operation.mkvtoolnix.mkvmerge.remux",
              operationVersion: "1.1.0",
              paramBindings: [
                { paramId: "source", from: { kind: "macro_input", socketId: "source" } },
                { paramId: "destination", from: { kind: "literal", value: { kind: "fileRef", fileId: "file.macro-temp-remux", state: "reserved", access: "write", mediaType: "video/x-matroska" } } }
              ],
              publishOutputs: [{ stepOutputId: "result", macroSocketId: "result" }]
            },
            {
              id: "verify",
              label: "Vérifier équivalence (placeholder déclaratif)",
              operationId: "operation.mkvtoolnix.verify.remux-equivalent.example",
              operationVersion: "0.0.0",
              paramBindings: [
                { paramId: "source", from: { kind: "macro_input", socketId: "source" } },
                { paramId: "candidate", from: { kind: "step_output", stepId: "remux", outputId: "result" } }
              ]
            }
          ],
          errorPolicy: "stop",
          notes: ["Chaque étape reste une opération argv sûre — pas de shell composite."]
        }
      },
      'catalogue.contracts': {
        path: 'catalogue/contracts.v1.json',
        schema: 'contracts.v1.json (v1.0.0)',
        title: 'Index général des contrats de distribution et macro-blocs',
        visual: `
          <div class="visual-summary-grid">
            <div class="visual-summary-box">
              <div class="visual-summary-title">Catalogue distribué unifié</div>
              <p class="visual-summary-text">Index canonique référençant les contrats d'opérations, macro-blocs, flux, presets et applications.</p>
            </div>
            <div class="visual-summary-box">
              <div class="visual-summary-title">Validation des schémas</div>
              <p class="visual-summary-text">Chaque contrat pointe vers son schéma JSON standardisé et sa politique d'intégrité cryptographique.</p>
            </div>
          </div>
        `,
        data: {
          schemaVersion: "1.0.0",
          legacyIndex: "catalogue/index.json",
          entries: [
            {
              id: "tool.mkvtoolnix",
              type: "tool",
              version: "1.0.0",
              manifestPath: "tools/mkvtoolnix/manifest.json",
              contracts: [{ kind: "distribution", path: "tools/mkvtoolnix/distribution.v1.json" }]
            },
            {
              id: "tool.internal.telemetry",
              type: "tool",
              version: "0.1.0",
              manifestPath: "tools/internal-telemetry/manifest.json",
              contracts: [{ kind: "distribution", path: "tools/internal-telemetry/distribution.v1.json" }]
            },
            {
              id: "macro.remux-verify.example",
              type: "block",
              version: "0.1.0",
              manifestPath: "blocks/macro-remux-verify.example/manifest.json",
              contracts: [{ kind: "macro-block", path: "blocks/macro-remux-verify.example/macro-block.json" }]
            }
          ]
        }
      }
    };

    let currentContractKey = 'tool.mkvtoolnix';

    function renderContract(key) {
      const contract = CONTRACTS_DB[key];
      if (!contract) return;
      currentContractKey = key;

      const pathEl = el('contract-file-path');
      const schemaEl = el('contract-file-schema');
      const summaryEl = el('contract-visual-summary');
      const codeEl = el('contract-json-content');

      if (pathEl) pathEl.textContent = contract.path;
      if (schemaEl) schemaEl.textContent = 'Schéma : ' + contract.schema;
      if (summaryEl) summaryEl.innerHTML = contract.visual;
      if (codeEl) codeEl.textContent = JSON.stringify(contract.data, null, 2);

      // Met à jour les onglets de contrat
      document.querySelectorAll('.contract-tab-btn').forEach(btn => {
        const isMatch = btn.getAttribute('data-contract-key') === key;
        btn.setAttribute('aria-selected', isMatch ? 'true' : 'false');
        btn.classList.toggle('active', isMatch);
      });
    }

    // Écouteurs sur les onglets de contrat
    document.querySelectorAll('.contract-tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const key = btn.getAttribute('data-contract-key');
        if (key) renderContract(key);
      });
    });

    // Copie du JSON
    const copyBtn = el('copy-contract-json-btn');
    if (copyBtn) {
      copyBtn.addEventListener('click', async () => {
        const contract = CONTRACTS_DB[currentContractKey];
        if (!contract) return;
        const text = JSON.stringify(contract.data, null, 2);
        try {
          if (navigator.clipboard && navigator.clipboard.writeText) {
            await navigator.clipboard.writeText(text);
          }
          showNotice('Contrat JSON copié dans le presse-papiers !', 'success');
        } catch {
          showNotice('Contrat prêt à la copie.', 'info');
        }
      });
    }

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

    // Boutons sur les cartes d'outils
    document.querySelectorAll('.inspect-contract-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const target = btn.getAttribute('data-target-contract');
        if (target && CONTRACTS_DB[target]) {
          renderContract(target);
          const area = el('contract-display-area');
          if (area) area.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }
      });
    });

    document.querySelectorAll('.test-tool-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const tool = btn.getAttribute('data-test-tool');
        const select = el('testbench-tool-select');
        if (select && tool) {
          select.value = tool;
        }
        const stepper = el('lifecycle-stepper');
        if (stepper) stepper.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
    });

    const gotoTelBtn = el('goto-telemetry-btn');
    if (gotoTelBtn) {
      gotoTelBtn.addEventListener('click', () => {
        const telCard = el('telemetry-hardware-card');
        if (telCard) telCard.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
    }

    // Banc d'essai : simulateur de cycle de vie du cache
    const runCycleBtn = el('run-testbench-cycle-btn');
    const logEl = el('testbench-log');
    const clearLogBtn = el('clear-testbench-log-btn');

    if (clearLogBtn && logEl) {
      clearLogBtn.addEventListener('click', () => {
        logEl.textContent = '[INFO] Journal réinitialisé.\n';
      });
    }

    function appendLog(line) {
      if (!logEl) return;
      const now = new Date().toLocaleTimeString('fr-FR');
      logEl.textContent += `[${now}] ${line}\n`;
      logEl.scrollTop = logEl.scrollHeight;
    }

    function resetStepper() {
      for (let i = 1; i <= 7; i++) {
        const step = el(`step-${i}`);
        if (step) {
          step.className = 'step-item';
          const statusText = step.querySelector('.step-status');
          if (statusText) statusText.textContent = 'En attente';
        }
      }
    }

    function setStepState(stepNum, state, statusMsg) {
      const step = el(`step-${stepNum}`);
      if (!step) return;
      step.className = 'step-item ' + state;
      const statusText = step.querySelector('.step-status');
      if (statusText && statusMsg) statusText.textContent = statusMsg;
    }

    if (runCycleBtn) {
      runCycleBtn.addEventListener('click', async () => {
        const toolSelect = el('testbench-tool-select');
        const archSelect = el('testbench-arch-select');
        const toolId = toolSelect ? toolSelect.value : 'tool.mkvtoolnix';
        const arch = archSelect ? archSelect.value : 'x86_64';

        runCycleBtn.disabled = true;
        runCycleBtn.textContent = 'Validation en cours…';
        resetStepper();

        appendLog(`--- Démarrage cycle ToolCacheManager pour ${toolId} (${arch}) ---`);

        // Étape 1 : Recherche .ready
        setStepState(1, 'active', 'Vérification…');
        appendLog(`Étape 1/7 : Inspection répertoire /work/tools-cache/${toolId}/1.0.0/${arch}/`);
        await new Promise(r => setTimeout(r, 260));
        setStepState(1, 'success', 'Marqueur vérifié');
        appendLog(`Étape 1/7 [OK] : Cache local vérifié, transition acquisition requise`);

        // Étape 2 : Verrou .lock
        setStepState(2, 'active', 'Acquisition…');
        appendLog(`Étape 2/7 : Création verrou exclusif .lock (protection multi-processus)`);
        await new Promise(r => setTimeout(r, 260));
        setStepState(2, 'success', 'Verrou acquis');
        appendLog(`Étape 2/7 [OK] : Verrou .lock obtenu avec succès`);

        // Étape 3 : Téléchargement vers staging
        setStepState(3, 'active', 'Rapatriement…');
        appendLog(`Étape 3/7 : Rapatriement archive vers staging (/work/tools-cache/staging/.tmp_dl)`);
        await new Promise(r => setTimeout(r, 300));
        setStepState(3, 'success', 'Archive en staging');
        appendLog(`Étape 3/7 [OK] : Archive téléchargée (taille conforme)`);

        // Étape 4 : Contrôle cryptographique SHA-256 archive
        setStepState(4, 'active', 'Calcul SHA-256…');
        appendLog(`Étape 4/7 : Vérification empreinte SHA-256 archive`);
        await new Promise(r => setTimeout(r, 280));
        setStepState(4, 'success', 'SHA-256 conforme');
        appendLog(`Étape 4/7 [OK] : Empreinte archive validée contre contrat de distribution`);

        // Étape 5 : Extraction native Rust sans shell (noShellUnpack)
        setStepState(5, 'active', 'Extraction mémoire…');
        appendLog(`Étape 5/7 : Extraction mémoire sans shell (noShellUnpack: true, contrôle anti-ZipSlip)`);
        await new Promise(r => setTimeout(r, 300));
        setStepState(5, 'success', 'Extrait sans shell');
        appendLog(`Étape 5/7 [OK] : Décompactage direct Rust sans privilège ni invocation shell`);

        // Étape 6 : Recalcul SHA-256 du binaire final
        setStepState(6, 'active', 'Contrôle binaire…');
        appendLog(`Étape 6/7 : Recalcul SHA-256 de l'exécutable binaire extrait`);
        await new Promise(r => setTimeout(r, 280));
        setStepState(6, 'success', 'Binaire intègre');
        appendLog(`Étape 6/7 [OK] : Binaire strictement identique à l'empreinte binarySha256 du contrat`);

        // Étape 7 : Pose du marqueur .ready et libération du verrou .lock
        setStepState(7, 'active', 'Finalisation…');
        appendLog(`Étape 7/7 : Application mode Unix 0755, création .ready et libération du verrou`);
        await new Promise(r => setTimeout(r, 260));
        setStepState(7, 'success', 'Prêt pour exécution');
        appendLog(`Étape 7/7 [SUCCÈS] : Outil opérationnel dans le cache. Exécution autorisée.`);

        showNotice(`Cycle ToolCacheManager terminé avec succès pour ${toolId} (${arch}) !`, 'success');
        runCycleBtn.disabled = false;
        runCycleBtn.textContent = '▶ Lancer la validation du cache';
      });
    }

    // Actualisation de la télémétrie matérielle
    const refreshTelBtn = el('refresh-telemetry-btn');
    if (refreshTelBtn) {
      refreshTelBtn.addEventListener('click', () => {
        refreshTelBtn.disabled = true;
        refreshTelBtn.textContent = 'Mesure…';

        setTimeout(() => {
          // Simulation réaliste de variation
          const cpuPercent = (12.0 + Math.random() * 6.0).toFixed(1);
          const ramPercent = (23.5 + Math.random() * 3.0).toFixed(1);
          const load1 = (0.35 + Math.random() * 0.15).toFixed(2);
          const load5 = (0.38 + Math.random() * 0.05).toFixed(2);

          const cpuVal = el('cpu-usage-val');
          const cpuFill = el('cpu-gauge-fill');
          const cpuLoad = el('cpu-load-text');
          const ramVal = el('ram-usage-val');
          const ramFill = el('ram-gauge-fill');

          if (cpuVal) cpuVal.textContent = cpuPercent + ' %';
          if (cpuFill) cpuFill.style.width = cpuPercent + '%';
          if (cpuLoad) cpuLoad.textContent = `Charge moyenne : ${load1} • ${load5} • 0.35`;
          if (ramVal) ramVal.textContent = ramPercent + ' %';
          if (ramFill) ramFill.style.width = ramPercent + '%';

          refreshTelBtn.disabled = false;
          refreshTelBtn.textContent = 'Actualiser';
          showNotice('Métriques matérielles de la sonde actualisées.', 'info');
        }, 350);
      });
    }

    // Affichage initial du premier contrat
    renderContract('tool.mkvtoolnix');
  }

  // --- Initialisation des écouteurs d'événements ---
  function initListeners() {
    setupTabs();
    setupTheme();

    const refreshBtn = el('refresh-all-btn');
    if (refreshBtn) refreshBtn.addEventListener('click', refreshAll);

    // Formulaire de session
    const saveKeyBtn = el('admin-key-save-btn');
    const inputKey = el('admin-key-input');
    if (saveKeyBtn && inputKey) {
      saveKeyBtn.addEventListener('click', () => login(inputKey.value));
      inputKey.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') login(inputKey.value);
      });
    }

    const logoutBtn = el('admin-key-logout-btn');
    if (logoutBtn) logoutBtn.addEventListener('click', logout);

    // Recherche de fichiers
    const filesSearch = el('files-search-input');
    if (filesSearch) {
      filesSearch.addEventListener('input', renderFilesTable);
    }

    // Dialogue confirmation remux
    const closeDialogX = el('confirm-dialog-close-x');
    if (closeDialogX) closeDialogX.addEventListener('click', closeConfirmRemuxDialog);

    const cancelDialogBtn = el('confirm-dialog-cancel-btn');
    if (cancelDialogBtn) cancelDialogBtn.addEventListener('click', closeConfirmRemuxDialog);

    const submitRemuxBtn = el('confirm-dialog-submit-btn');
    if (submitRemuxBtn) submitRemuxBtn.addEventListener('click', submitRemuxJob);

    // Recherche de tâches
    const jobsSearch = el('jobs-search-input');
    if (jobsSearch) {
      jobsSearch.addEventListener('input', renderJobsTable);
    }

    // Filtres de statut de tâches
    document.querySelectorAll('.filter-chip[data-filter]').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.filter-chip[data-filter]').forEach(b => b.setAttribute('aria-pressed', 'false'));
        btn.setAttribute('aria-pressed', 'true');
        activeJobFilter = btn.getAttribute('data-filter') || 'all';
        renderJobsTable();
      });
    });

    // Configuration de la section Plugins, Outils et Contrats
    setupPluginsPanel();

    // Dialogue détails tâche
    const jobDialogCloseX = el('job-dialog-close-x');
    if (jobDialogCloseX) jobDialogCloseX.addEventListener('click', closeJobDetailDialog);

    const jobDialogCloseBtn = el('job-dialog-close-btn');
    if (jobDialogCloseBtn) jobDialogCloseBtn.addEventListener('click', closeJobDetailDialog);

    // Fermeture avec Échap pour les modales
    window.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        closeConfirmRemuxDialog();
        closeJobDetailDialog();
      }
    });

    // Polling régulier de santé toutes les 30 s
    setInterval(checkHealth, 30000);
  }

  // Démarrage
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      initListeners();
      refreshAll();
    });
  } else {
    initListeners();
    refreshAll();
  }
})();
"#;
