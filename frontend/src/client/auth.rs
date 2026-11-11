//! Authentification et gestion de session.

pub const SCRIPT: &str = r###"
// --- Gestion de la session et de l'authentification (Contrat #29) ---
  function updateSessionUI() {
    const pill = el('session-status-pill');
    const label = el('session-status-text');
    const authStatusText = el('auth-status-text');
    const logoutBtn = el('admin-key-logout-btn');
    const saveBtn = el('admin-key-save-btn');
    const input = el('admin-key-input');

    if (sessionMode === 'cookie_session') {
      if (typeof startJobsSse === 'function') startJobsSse();
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
      if (typeof stopJobsSse === 'function') stopJobsSse();
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
      if (typeof stopJobsSse === 'function') stopJobsSse();
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

"###;
