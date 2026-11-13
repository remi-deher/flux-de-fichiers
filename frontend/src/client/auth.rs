//! Accès : installation de la centrale, connexion, session web et droits de l'utilisateur (ADR 0011).

pub const SCRIPT: &str = r###"
// --- Accès : installation, connexion, session et droits ---
  let currentUser = null;

  const ROLE_LABELS = { admin: 'Administrateur', worker: 'Worker' };
  const AUTH_ERROR_MESSAGES = {
    unauthenticated: 'Identifiant ou mot de passe incorrect.',
    too_many_attempts: 'Trop d\'essais : réessayez dans quelques minutes.',
    invalid_input: 'Vérifiez les champs : identifiant de 3 à 40 caractères (lettres, chiffres, . - _), mot de passe d\'au moins 8 caractères.',
    already_configured: 'La centrale est déjà installée : connectez-vous.',
    forbidden: 'Requête refusée (origine non autorisée).'
  };

  function authErrorMessage(code) {
    return AUTH_ERROR_MESSAGES[code] || 'Opération impossible. Réessayez.';
  }

  function setAuthGateError(message) {
    const node = el('auth-gate-error');
    if (node) node.textContent = message || '';
  }

  // Affiche l'écran d'accès : kind vaut 'setup' (installation) ou 'login' (connexion).
  function showAuthGate(kind, instanceName) {
    const gate = el('auth-gate');
    const shell = el('application-shell');
    const setupForm = el('setup-form');
    const loginForm = el('login-form');
    if (typeof stopJobsSse === 'function') stopJobsSse();
    if (shell) shell.hidden = true;
    if (gate) gate.hidden = false;
    if (setupForm) setupForm.hidden = kind !== 'setup';
    if (loginForm) loginForm.hidden = kind !== 'login';
    const title = el('auth-gate-title');
    const subtitle = el('auth-gate-subtitle');
    if (title) title.textContent = kind === 'setup' ? 'Installation de la centrale' : 'Connexion';
    if (subtitle) {
      subtitle.textContent = kind === 'setup'
        ? 'Créez le compte administrateur pour commencer.'
        : (instanceName || '');
    }
    setAuthGateError('');
    const focusTarget = el(kind === 'setup' ? 'setup-name' : 'login-username');
    if (focusTarget) focusTarget.focus();
  }

  function hideAuthGate() {
    const gate = el('auth-gate');
    const shell = el('application-shell');
    if (gate) gate.hidden = true;
    if (shell) shell.hidden = false;
  }

  // Un écran est accessible si ce nœud l'héberge et si l'utilisateur a le droit exigé.
  function isTabAllowed(slug, me) {
    const access = PANEL_ACCESS[slug];
    if (!access || !me) return true;
    return access.hostedOn.includes(me.nodeKind) && me.permissions.includes(access.permission);
  }

  function renderAccount(me) {
    const setText = (id, text) => {
      const node = el(id);
      if (node) node.textContent = text;
    };
    setText('account-name-text', me.name);
    setText('account-role-text', ROLE_LABELS[me.role] || me.role);
    setText('account-instance-text', me.instanceName || '—');
    if (me.instanceName) document.title = `${me.instanceName} — Flux de Fichiers`;
  }

  // Masque les écrans interdits (la même interface sert tous les rôles et tous les types de nœud).
  function applyAccess(me) {
    currentUser = me;
    sessionMode = 'cookie_session';
    TABS.forEach(slug => {
      const allowed = isTabAllowed(slug, me);
      const button = el(`tab-btn-${slug}`);
      const panel = el(`panel-${slug}`);
      if (button) button.hidden = !allowed;
      if (panel && !allowed) panel.hidden = true;
    });
    const firstAllowed = TABS.find(slug => isTabAllowed(slug, me));
    if (firstAllowed && !isTabAllowed(activeTab, me)) switchTab(firstAllowed);
    renderAccount(me);
    if (typeof startJobsSse === 'function') startJobsSse();
  }

  async function fetchJson(url) {
    try {
      const res = await fetch(url, { credentials: 'include' });
      return res.ok ? await res.json() : null;
    } catch {
      return null;
    }
  }

  // Au chargement : installation à faire, sinon session existante, sinon connexion.
  async function bootstrapAuth() {
    const status = await fetchJson('/api/v1/setup/status');
    if (!status) {
      showAuthGate('login');
      setAuthGateError('Centrale injoignable.');
      return false;
    }
    if (status.needsSetup) {
      showAuthGate('setup');
      return false;
    }
    const session = await fetchJson('/api/v1/auth/session');
    if (session) {
      csrfToken = session.csrf_token || '';
      const me = await fetchJson('/api/v1/me');
      if (me) {
        applyAccess(me);
        hideAuthGate();
        return true;
      }
    }
    showAuthGate('login', status.instanceName);
    return false;
  }

  async function openSession(url, payload, errorFallback) {
    setAuthGateError('');
    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify(payload)
      });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) {
        setAuthGateError(authErrorMessage(body.error && body.error.code) || errorFallback);
        return false;
      }
      csrfToken = body.csrf_token || '';
      const me = await fetchJson('/api/v1/me');
      if (!me) {
        setAuthGateError(errorFallback);
        return false;
      }
      applyAccess(me);
      hideAuthGate();
      await loadAllData();
      return true;
    } catch {
      setAuthGateError('Centrale injoignable.');
      return false;
    }
  }

  async function submitSetup(event) {
    event.preventDefault();
    const password = el('setup-password').value;
    if (password !== el('setup-password-confirm').value) {
      setAuthGateError('Les deux mots de passe ne correspondent pas.');
      return;
    }
    await openSession('/api/v1/setup/central', {
      name: el('setup-name').value,
      username: el('setup-username').value,
      password
    }, 'Installation impossible.');
    el('setup-password').value = '';
    el('setup-password-confirm').value = '';
  }

  async function submitLogin(event) {
    event.preventDefault();
    await openSession('/api/v1/auth/login', {
      username: el('login-username').value,
      password: el('login-password').value
    }, 'Connexion impossible.');
    el('login-password').value = '';
  }

  async function logout() {
    const instanceName = currentUser ? currentUser.instanceName : '';
    try {
      const headers = csrfToken ? { 'x-csrf-token': csrfToken } : {};
      await fetch('/api/v1/auth/logout', { method: 'POST', headers, credentials: 'include' });
    } catch {}
    currentUser = null;
    csrfToken = '';
    sessionMode = 'none';
    showAuthGate('login', instanceName);
  }

  async function changePassword(event) {
    event.preventDefault();
    const current = el('account-current-password');
    const next = el('account-new-password');
    try {
      await apiFetch('/api/v1/account/password', {
        method: 'PUT',
        body: JSON.stringify({ current_password: current.value, new_password: next.value })
      });
      showNotice('Mot de passe modifié. Vos autres sessions sont fermées.', 'success');
      current.value = '';
      next.value = '';
    } catch (e) {
      showNotice(e.message === 'Non autorisé' ? 'Mot de passe actuel incorrect.' : authErrorMessage(e.message), 'error');
    }
  }
"###;
