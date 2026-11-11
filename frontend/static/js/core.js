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
