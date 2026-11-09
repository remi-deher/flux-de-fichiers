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
  const TABS = ['dashboard', 'files', 'jobs', 'workers', 'settings'];

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
    document.querySelectorAll('.filter-chip').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.filter-chip').forEach(b => b.setAttribute('aria-pressed', 'false'));
        btn.setAttribute('aria-pressed', 'true');
        activeJobFilter = btn.getAttribute('data-filter') || 'all';
        renderJobsTable();
      });
    });

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
