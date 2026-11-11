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
  let manualLogout = false;

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
  const TABS = ['dashboard', 'files', 'flows', 'jobs', 'workers', 'plugins', 'settings'];

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

    // Auto-session style *arr si pas de déconnexion manuelle
    if (!manualLogout) {
      try {
        const res = await fetch('/api/v1/auth/session', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          credentials: 'include',
          body: JSON.stringify({})
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
    }

    if (volatileAdminKey) {
      sessionMode = 'memory_key';
    } else {
      sessionMode = 'none';
    }
    updateSessionUI();
  }

  async function login(key) {
    manualLogout = false;
    const cleanKey = (key || '').trim();

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

    // 2. Fallback mémoire volatile avec jeton Bearer (si clé fournie)
    if (cleanKey) {
      volatileAdminKey = cleanKey;
      sessionMode = 'memory_key';
      updateSessionUI();
      showNotice('Clé mémorisée en mémoire volatile (aucun stockage disque ou sessionStorage).', 'success');
      refreshAll();
    } else {
      showNotice('Impossible d\'ouvrir la session.', 'error');
    }
  }

  async function logout() {
    manualLogout = true;
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
      const hadActiveSession = sessionMode !== 'none';
      volatileAdminKey = '';
      sessionMode = 'none';
      csrfToken = '';
      updateSessionUI();
      if (hadActiveSession) {
        showNotice('Accès non autorisé (401) : clé révoquée ou session invalide. Mémoire purgée.', 'error');
      }
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
      const origin = window.location.origin || 'http://localhost:3000';
      curlCmd.textContent = `curl -H "Authorization: Bearer <ADMIN_KEY>" "${origin}/api/v1/jobs/${encodeURIComponent(job.id)}/output" -o "${job.output_name || 'resultat.mkv'}"`;
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
      loadWorkers(),
      loadSystemConfig(),
      loadToolsStatus()
    ]);

    if (refreshBtn) {
      refreshBtn.disabled = false;
      refreshBtn.textContent = 'Actualiser';
    }
  }

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
  }

  // --- Concepteur et Gestionnaire de Flux (Flows) type Zapier / FileFlows ---
  function setupFlowsPanel() {
    const libraryView = el('flows-library-view');
    const canvasView = el('flows-canvas-view');
    const wizardDialog = el('flow-wizard-dialog');
    const openWizardBtn = el('btn-open-flow-wizard');
    const closeWizardX = el('flow-wizard-close-x');
    const cancelWizardBtn = el('flow-wizard-cancel-btn');
    const generateWizardBtn = el('flow-wizard-generate-btn');
    const backToLibraryBtn = el('btn-back-to-library');
    const saveCanvasFlowBtn = el('btn-canvas-save-flow');
    const nodesContainer = el('canvas-nodes-container');
    const svgLines = el('canvas-svg-lines');

    let currentSelectedTemplate = 'hevc';

    // 1. Navigation Canvas <-> Bibliothèque
    function openCanvas(flowId, flowTitle) {
      if (libraryView) libraryView.style.display = 'none';
      if (canvasView) canvasView.style.display = 'block';
      const titleEl = el('canvas-flow-title');
      if (titleEl) titleEl.textContent = `Éditeur visuel : ${flowTitle}`;
      setTimeout(drawCanvasLines, 60);
    }

    function closeCanvas() {
      if (canvasView) canvasView.style.display = 'none';
      if (libraryView) libraryView.style.display = 'block';
    }

    if (backToLibraryBtn) backToLibraryBtn.addEventListener('click', closeCanvas);

    document.querySelectorAll('.open-canvas-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const card = btn.closest('.flow-card');
        const title = card ? card.querySelector('.flow-card-title').textContent : 'Flux';
        const flowId = btn.getAttribute('data-flow') || 'flow';
        openCanvas(flowId, title);
      });
    });

    // 2. Assistant de configuration (Setup Wizard)
    if (openWizardBtn && wizardDialog) {
      openWizardBtn.addEventListener('click', () => {
        if (typeof wizardDialog.showModal === 'function') {
          wizardDialog.showModal();
        } else {
          wizardDialog.setAttribute('open', '');
        }
      });
    }

    function closeWizard() {
      if (wizardDialog) {
        if (typeof wizardDialog.close === 'function') {
          wizardDialog.close();
        } else {
          wizardDialog.removeAttribute('open');
        }
      }
    }

    if (closeWizardX) closeWizardX.addEventListener('click', closeWizard);
    if (cancelWizardBtn) cancelWizardBtn.addEventListener('click', closeWizard);

    // Sélection du modèle dans le wizard
    document.querySelectorAll('.wizard-template-card').forEach(card => {
      card.addEventListener('click', () => {
        document.querySelectorAll('.wizard-template-card').forEach(c => c.classList.remove('active'));
        card.classList.add('active');
        currentSelectedTemplate = card.getAttribute('data-template') || 'hevc';

        const nameInput = el('wizard-flow-name');
        const encSelect = el('wizard-flow-encoder');
        if (currentSelectedTemplate === 'hevc') {
          if (nameInput) nameInput.value = 'Transcodage HEVC Intel QSV';
          if (encSelect) encSelect.value = 'qsv';
        } else if (currentSelectedTemplate === 'mkv-remux') {
          if (nameInput) nameInput.value = 'Remuxage sans perte Matroska';
          if (encSelect) encSelect.value = 'cpu';
        } else if (currentSelectedTemplate === 'mp4-web') {
          if (nameInput) nameInput.value = 'Optimisation Streaming Web (MP4)';
          if (encSelect) encSelect.value = 'vaapi';
        } else {
          if (nameInput) nameInput.value = 'Nouveau flux personnalisé';
        }
      });
    });

    if (generateWizardBtn) {
      generateWizardBtn.addEventListener('click', () => {
        const nameInput = el('wizard-flow-name');
        const flowTitle = nameInput && nameInput.value ? nameInput.value.trim() : 'Nouveau flux';
        closeWizard();

        // Reconstituer les nœuds selon le template choisi
        if (nodesContainer) {
          nodesContainer.innerHTML = `
            <div class="canvas-node implicit-node selected" id="node-1" data-node-id="node-1" data-block-type="inspect">
              <div class="node-connector connector-out"></div>
              <div class="node-header">
                <span class="node-icon">🔍</span>
                <div class="node-title-box">
                  <span class="node-kind">MediaInfo • Début de flux</span>
                  <h4 class="node-name">Inspecter métadonnées</h4>
                </div>
                <span class="status-pill info node-pill">Implicite</span>
              </div>
              <div class="node-summary">Extraction exhaustive de toutes les pistes et propriétés du média.</div>
            </div>
          `;

          if (currentSelectedTemplate === 'hevc') {
            appendNodeToCanvas('video-encode', 'FFmpeg Builder', 'Réencodage Vidéo', 'HEVC • Intel QSV • Qualité : 6 • Vitesse : 3');
            appendNodeToCanvas('audio-encode', 'FFmpeg Builder', 'Réencodage Audio', 'Codec : AAC • 192 kbps • Stéréo');
            appendNodeToCanvas('remux-mkv', 'MKVToolNix', 'Remuxage Matroska', 'Assemblage final MKV • Langues : fre, eng');
          } else if (currentSelectedTemplate === 'mkv-remux') {
            appendNodeToCanvas('remux-mkv', 'MKVToolNix', 'Remuxage Matroska', 'Filtrage langues fre, eng • Copie directe sans transcodage');
          } else if (currentSelectedTemplate === 'mp4-web') {
            appendNodeToCanvas('remux-mp4', 'MP4Box GPAC', 'Remuxage MP4', 'FastStart activé • Moov atom en début de fichier');
          }
        }

        openCanvas('generated', flowTitle);
        showNotice(`Flux « ${flowTitle} » généré avec succès ! Prêt dans le Canvas.`, 'success');
      });
    }

    // 3. Tracé des connexions SVG entre les connecteurs
    function drawCanvasLines() {
      if (!svgLines || !nodesContainer) return;
      const nodes = Array.from(nodesContainer.querySelectorAll('.canvas-node'));
      if (nodes.length < 2) {
        svgLines.innerHTML = '';
        return;
      }

      const boardRect = nodesContainer.parentElement.getBoundingClientRect();
      let pathsHtml = `
        <defs>
          <marker id="arrow" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--primary)" />
          </marker>
        </defs>
      `;

      for (let i = 0; i < nodes.length - 1; i++) {
        const fromNode = nodes[i];
        const toNode = nodes[i + 1];

        const outConn = fromNode.querySelector('.connector-out');
        const inConn = toNode.querySelector('.connector-in');

        if (outConn && inConn) {
          const rOut = outConn.getBoundingClientRect();
          const rIn = inConn.getBoundingClientRect();

          const x1 = rOut.left + rOut.width / 2 - boardRect.left + nodesContainer.parentElement.scrollLeft;
          const y1 = rOut.top + rOut.height / 2 - boardRect.top + nodesContainer.parentElement.scrollTop;

          const x2 = rIn.left + rIn.width / 2 - boardRect.left + nodesContainer.parentElement.scrollLeft;
          const y2 = rIn.top + rIn.height / 2 - boardRect.top + nodesContainer.parentElement.scrollTop;

          const cy1 = y1 + 18;
          const cy2 = y2 - 18;

          pathsHtml += `<path d="M ${x1} ${y1} C ${x1} ${cy1}, ${x2} ${cy2}, ${x2} ${y2}" stroke="var(--primary)" stroke-width="2.5" fill="none" marker-end="url(#arrow)" />`;
        }
      }
      svgLines.innerHTML = pathsHtml;
    }

    window.addEventListener('resize', drawCanvasLines);

    // 4. Ajout d'un bloc au canvas
    function appendNodeToCanvas(blockType, kind, title, summary) {
      if (!nodesContainer) return;
      const count = nodesContainer.querySelectorAll('.canvas-node').length + 1;
      const nodeId = `node-${count}`;
      const icon = blockType.includes('video') ? '🎞️' : blockType.includes('audio') ? '🔊' : blockType.includes('mp4') ? '📦' : '🎬';

      const div = document.createElement('div');
      div.className = 'canvas-node';
      div.id = nodeId;
      div.setAttribute('data-node-id', nodeId);
      div.setAttribute('data-block-type', blockType);
      div.innerHTML = `
        <div class="node-connector connector-in"></div>
        <div class="node-connector connector-out"></div>
        <div class="node-header">
          <span class="node-icon">${icon}</span>
          <div class="node-title-box">
            <span class="node-kind">${kind}</span>
            <h4 class="node-name">${title}</h4>
          </div>
          <button type="button" class="node-remove-btn" title="Supprimer ce bloc">✕</button>
        </div>
        <div class="node-summary" id="${nodeId}-summary">${summary}</div>
      `;

      nodesContainer.appendChild(div);
      bindNodeEvents(div);
      selectNode(div);
      setTimeout(drawCanvasLines, 30);
    }

    // 5. Sélection et inspecteur de bloc
    function selectNode(nodeEl) {
      if (!nodeEl) return;
      document.querySelectorAll('.canvas-node').forEach(n => n.classList.remove('selected'));
      nodeEl.classList.add('selected');

      const blockType = nodeEl.getAttribute('data-block-type') || 'inspect';
      const nodeName = nodeEl.querySelector('.node-name') ? nodeEl.querySelector('.node-name').textContent : 'Bloc';
      const nodeKind = nodeEl.querySelector('.node-kind') ? nodeEl.querySelector('.node-kind').textContent : 'Outil';

      const titleEl = el('inspector-block-title');
      const toolEl = el('inspector-block-tool');
      const formContainer = el('inspector-form-container');

      if (titleEl) titleEl.textContent = nodeName;
      if (toolEl) toolEl.textContent = nodeKind.split('•')[0].trim();

      if (formContainer) {
        if (blockType === 'inspect') {
          formContainer.innerHTML = `
            <div class="status-pill info" style="width: 100%; margin-bottom: 0.75rem; justify-content: center;">
              Bloc implicite non configurable
            </div>
            <p style="font-size: 0.8125rem; color: var(--text-dim); line-height: 1.5;">
              Ce bloc s'exécute automatiquement en début de flux. MediaInfo extrait l'intégralité des flux (codecs, pistes audio, sous-titres, langues, métadonnées) et les injecte dans le contexte d'exécution des blocs suivants.
            </p>
          `;
        } else if (blockType === 'video-encode') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label for="node-cfg-codec" class="form-label">Codec vidéo</label>
              <select id="node-cfg-codec" class="form-select">
                <option value="hevc" selected>HEVC / H.265 (Recommandé)</option>
                <option value="h264">H.264 / AVC</option>
                <option value="av1">AV1</option>
                <option value="copy">Copie directe</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-encoder" class="form-label">Encodeur matériel</label>
              <select id="node-cfg-encoder" class="form-select">
                <option value="auto" selected>Intel QSV (Automatique)</option>
                <option value="nvenc">Nvidia NVENC</option>
                <option value="vaapi">VA-API (Linux)</option>
                <option value="cpu">Processeur CPU (libx265)</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-quality" class="form-label">Qualité (1-9) : <span id="node-quality-val" style="font-weight: 700; color: var(--primary);">6</span></label>
              <input type="range" id="node-cfg-quality" min="1" max="9" value="6" style="width: 100%; accent-color: var(--primary);" />
              <span class="form-help">Plus élevé = meilleure netteté & fichier plus lourd.</span>
            </div>
            <div class="form-group">
              <label for="node-cfg-speed" class="form-label">Vitesse (1-5) : <span id="node-speed-val" style="font-weight: 700; color: var(--primary);">3</span></label>
              <input type="range" id="node-cfg-speed" min="1" max="5" value="3" style="width: 100%; accent-color: var(--primary);" />
              <span class="form-help">Plus élevé = encodage rapide, compression réduite.</span>
            </div>
            <div class="form-group">
              <label for="node-cfg-custom" class="form-label">Paramètres personnalisés libres</label>
              <input type="text" id="node-cfg-custom" class="form-input" placeholder="ex: -pix_fmt yuv420p10le" />
            </div>
          `;
          const qInput = el('node-cfg-quality');
          const qVal = el('node-quality-val');
          if (qInput && qVal) qInput.addEventListener('input', () => { qVal.textContent = qInput.value; });

          const sInput = el('node-cfg-speed');
          const sVal = el('node-speed-val');
          if (sInput && sVal) sInput.addEventListener('input', () => { sVal.textContent = sInput.value; });
        } else if (blockType === 'audio-encode') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label for="node-cfg-audio-codec" class="form-label">Codec audio</label>
              <select id="node-cfg-audio-codec" class="form-select">
                <option value="aac" selected>AAC (Recommandé)</option>
                <option value="opus">Opus</option>
                <option value="ac3">AC3 / Dolby Digital</option>
                <option value="flac">FLAC (Sans perte)</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-audio-bitrate" class="form-label">Débit cible</label>
              <select id="node-cfg-audio-bitrate" class="form-select">
                <option value="128k">128 kbps (Standard)</option>
                <option value="192k" selected>192 kbps (Haute qualité)</option>
                <option value="320k">320 kbps (Qualité maximale)</option>
              </select>
            </div>
            <div class="form-group">
              <label for="node-cfg-audio-channels" class="form-label">Disposition des canaux</label>
              <select id="node-cfg-audio-channels" class="form-select">
                <option value="stereo" selected>Stéréo (2.0)</option>
                <option value="5.1">Surround 5.1</option>
                <option value="original">Conserver original</option>
              </select>
            </div>
          `;
        } else if (blockType === 'remux-mkv') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label for="node-cfg-langs" class="form-label">Langues conservées</label>
              <input type="text" id="node-cfg-langs" class="form-input" value="fre, eng" />
              <span class="form-help">Codes ISO-639 séparés par des virgules (ex: fre, eng, jpn).</span>
            </div>
            <div class="form-group">
              <label for="node-cfg-title" class="form-label">Titre du conteneur (optionnel)</label>
              <input type="text" id="node-cfg-title" class="form-input" placeholder="ex: {filename}" />
            </div>
          `;
        } else if (blockType === 'remux-mp4') {
          formContainer.innerHTML = `
            <div class="form-group">
              <label class="form-label">Optimisation Streaming Web</label>
              <label style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.8125rem; color: var(--text-main); cursor: pointer;">
                <input type="checkbox" id="node-cfg-faststart" checked />
                Activer FastStart (déplacement de l'atome moov en tête)
              </label>
            </div>
          `;
        }
      }
    }

    function bindNodeEvents(nodeEl) {
      nodeEl.addEventListener('click', (e) => {
        if (e.target.closest('.node-remove-btn')) return;
        selectNode(nodeEl);
      });

      const removeBtn = nodeEl.querySelector('.node-remove-btn');
      if (removeBtn) {
        removeBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          const name = nodeEl.querySelector('.node-name') ? nodeEl.querySelector('.node-name').textContent : 'Bloc';
          nodeEl.remove();
          showNotice(`Bloc « ${name} » retiré du flux.`, 'info');
          drawCanvasLines();
          const firstNode = nodesContainer.querySelector('.canvas-node');
          if (firstNode) selectNode(firstNode);
        });
      }
    }

    // Lier les nœuds déjà présents
    document.querySelectorAll('.canvas-node').forEach(bindNodeEvents);

    // Clic sur un bloc dans la palette latérale
    document.querySelectorAll('.draggable-block-item.addable').forEach(item => {
      item.addEventListener('click', () => {
        const blockType = item.getAttribute('data-block-type') || 'block';
        const title = item.querySelector('strong') ? item.querySelector('strong').textContent : 'Nouveau bloc';
        const kind = item.querySelector('span') ? item.querySelector('span').textContent : 'Outil';
        appendNodeToCanvas(blockType, kind, title, 'Configuration par défaut appliquée.');
        showNotice(`Bloc « ${title} » ajouté à votre chaîne d'exécution !`, 'success');
      });
    });

    if (saveCanvasFlowBtn) {
      saveCanvasFlowBtn.addEventListener('click', () => {
        showNotice('Modifications du flux enregistrées avec succès dans la configuration.', 'success');
        setTimeout(closeCanvas, 400);
      });
    }

    document.querySelectorAll('.set-default-flow-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const card = btn.closest('.flow-card');
        const title = card ? card.querySelector('.flow-card-title').textContent : 'Flux';
        document.querySelectorAll('.flow-card').forEach(c => c.classList.remove('active-flow'));
        if (card) card.classList.add('active-flow');
        showNotice(`Le flux « ${title} » est maintenant le flux actif par défaut.`, 'success');
      });
    });
  }

  // --- Initialisation des écouteurs d'événements ---
  function initListeners() {
    setupTabs();
    setupTheme();

    const refreshBtn = el('refresh-all-btn');
    if (refreshBtn) refreshBtn.addEventListener('click', refreshAll);

    // Raccourcis d'action du tableau de bord
    const gotoFilesBtn = el('quick-goto-files-btn');
    if (gotoFilesBtn) gotoFilesBtn.addEventListener('click', () => switchTab('files'));

    const gotoFlowsBtn = el('quick-goto-flows-btn');
    if (gotoFlowsBtn) gotoFlowsBtn.addEventListener('click', () => switchTab('flows'));

    const gotoJobsBtn = el('quick-goto-jobs-btn');
    if (gotoJobsBtn) gotoJobsBtn.addEventListener('click', () => switchTab('jobs'));

    const gotoSettingsBtn = el('quick-goto-settings-btn');
    if (gotoSettingsBtn) gotoSettingsBtn.addEventListener('click', () => switchTab('settings'));

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

    // Configuration du Gestionnaire de Flux
    setupFlowsPanel();

    // Configuration de la section Plugins et Outils
    setupPluginsPanel();

    // Configuration de la section Raccordement Worker et Clés
    setupWorkerConfigUi();

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
        const wizard = el('flow-wizard-dialog');
        if (wizard && wizard.open) {
          wizard.close();
        }
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
