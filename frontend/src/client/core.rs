//! Noyau et utilitaires clients.

pub const SCRIPT: &str = r###"
// Sécurité : la clé d'administration réside UNIQUEMENT en mémoire volatile JavaScript de l'onglet actif.
  // Aucun stockage en clair dans sessionStorage ni localStorage.
  let volatileAdminKey = '';
  let csrfToken = '';
  let activeTab = 'dashboard';
  let cachedFiles = [];
  let cachedJobs = [];
  let cachedWorkers = [];
  let activeJobFilter = 'all';
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

  function renderStatusPill(label, variant = 'info', options = {}) {
    const dot = options.noDot ? '' : `<span class="status-dot${options.pulse ? ' pulse' : ''}"></span>`;
    const extraClass = options.extraClass ? ' ' + options.extraClass : '';
    const titleAttr = options.title ? ` title="${escapeHtml(options.title)}"` : '';
    return `<span class="status-pill ${variant}${extraClass}"${titleAttr}>${dot}${label}</span>`;
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

  // --- Suivi du nombre de workers en ligne (central inclus) ---
  let isCoordinatorOnline = false;
  let remoteActiveWorkersCount = 0;

  function updateHeaderWorkersCount(isOnline, remoteCount) {
    if (typeof isOnline === 'boolean') isCoordinatorOnline = isOnline;
    if (typeof remoteCount === 'number') remoteActiveWorkersCount = remoteCount;

    const pill = el('header-workers-pill');
    const label = el('header-workers-count-text');
    if (!pill || !label) return;

    if (!isCoordinatorOnline) {
      pill.className = 'status-pill offline';
      pill.setAttribute('aria-label', 'Coordinateur et workers hors ligne');
      pill.setAttribute('title', 'Le coordinateur central est inaccessible');
      label.textContent = 'Hors ligne';
      return;
    }

    const total = 1 + remoteActiveWorkersCount;
    pill.className = 'status-pill online';
    const text = total === 1 ? '1 worker en ligne' : `${total} workers en ligne`;
    pill.setAttribute('aria-label', `${text} (coordinateur central inclus)`);
    pill.setAttribute('title', `${text} (1 coordinateur central autonome + ${remoteActiveWorkersCount} distant(s))`);
    label.textContent = text;
  }

  // --- Calcul des statistiques pour le Hub d'accueil (Vue d'ensemble) ---
  function updateOverviewStats() {
    // 1. Calcul du volume total des fichiers gérés
    const totalBytes = cachedFiles.reduce((acc, f) => acc + (f.size || 0), 0);
    const metricStorage = el('metric-storage-volume');
    if (metricStorage) metricStorage.textContent = formatBytes(totalBytes);

    // 2. Calcul du taux de succès des tâches
    const finished = cachedJobs.filter(j => j.status === 'succeeded' || j.status === 'failed');
    const succeeded = cachedJobs.filter(j => j.status === 'succeeded').length;
    const metricSuccess = el('metric-success-rate');
    if (metricSuccess) {
      const hasFinished = finished.length > 0;
      const rate = hasFinished ? Math.round((succeeded / finished.length) * 100) : null;
      metricSuccess.textContent = hasFinished ? `${rate}%` : '—';
      const card = metricSuccess.closest('.metric-card');
      if (card) card.dataset.tone = successRateTone(rate);
    }

    // 3. Distribution des formats de conteneurs médias
    const totalFiles = cachedFiles.length;
    const totalLabel = el('media-formats-total-label');
    if (totalLabel) {
      totalLabel.textContent = `${totalFiles} fichier${totalFiles > 1 ? 's' : ''}`;
    }

    const bar = el('media-formats-bar');
    const legend = el('media-formats-legend');
    if (totalFiles > 0 && bar && legend) {
      let mkvCount = 0;
      let mp4Count = 0;
      let otherCount = 0;

      cachedFiles.forEach(f => {
        const name = (f.name || '').toLowerCase();
        if (name.endsWith('.mkv')) mkvCount++;
        else if (name.endsWith('.mp4') || name.endsWith('.m4v')) mp4Count++;
        else otherCount++;
      });

      const mkvPct = Math.round((mkvCount / totalFiles) * 100);
      const mp4Pct = Math.round((mp4Count / totalFiles) * 100);
      const otherPct = Math.max(0, 100 - mkvPct - mp4Pct);

      bar.innerHTML = `
        <div class="segment-fill mkv" style="width: ${mkvPct}%;" title="MKV: ${mkvCount} (${mkvPct}%)"></div>
        <div class="segment-fill mp4" style="width: ${mp4Pct}%;" title="MP4: ${mp4Count} (${mp4Pct}%)"></div>
        <div class="segment-fill other" style="width: ${otherPct}%;" title="Autres: ${otherCount} (${otherPct}%)"></div>
      `;

      legend.innerHTML = `
        <span class="legend-item"><span class="legend-dot mkv"></span>MKV: ${mkvPct}% (${mkvCount})</span>
        <span class="legend-item"><span class="legend-dot mp4"></span>MP4: ${mp4Pct}% (${mp4Count})</span>
        <span class="legend-item"><span class="legend-dot other"></span>Autres: ${otherPct}% (${otherCount})</span>
      `;
    }
  }

  // --- Ton de la tuile « Taux de réussite » selon le pourcentage de tâches réussies ---
  const SUCCESS_RATE_GOOD_MIN = 90;
  const SUCCESS_RATE_WARNING_MIN = 60;

  function successRateTone(rate) {
    if (rate === null) return 'neutral';
    if (rate >= SUCCESS_RATE_GOOD_MIN) return 'success';
    if (rate >= SUCCESS_RATE_WARNING_MIN) return 'warning';
    return 'danger';
  }

  // --- Bandeau d'état de la vue d'ensemble ---
  function setHeroStatus(reachable) {
    const badge = el('hero-status-badge');
    const text = el('hero-status-text');
    if (badge) badge.dataset.tone = reachable ? 'success' : 'danger';
    if (text) text.textContent = reachable ? 'Coordinateur joignable' : 'Coordinateur inaccessible';
  }

  // --- Vérification de santé ---
  async function checkHealth() {
    const metricHealth = el('metric-health');

    try {
      const res = await fetch('/health');
      if (res.status === 204 || res.ok) {
        updateHeaderWorkersCount(true);
        setHeroStatus(true);
        if (metricHealth) metricHealth.textContent = 'Joignable';
        return true;
      } else {
        throw new Error('Code HTTP ' + res.status);
      }
    } catch (e) {
      updateHeaderWorkersCount(false, 0);
      setHeroStatus(false);
      if (metricHealth) metricHealth.textContent = 'Inaccessible';
      return false;
    }
  }

"###;
