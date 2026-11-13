//! Clés d'API : registre, création (clé affichée une seule fois) et révocation.

pub const SCRIPT: &str = r###"
// --- Clés d'API ---
  let cachedApiKeys = [];
  let selectedApiKeyId = null;
  const API_KEY_ROLE_LABELS = { admin: 'Administrateur', worker: 'Worker' };

  function setHidden(ids, hidden) {
    ids.forEach(id => {
      const node = el(id);
      if (node) node.hidden = hidden;
    });
  }

  function apiKeyState(key) {
    return key.revokedAt ? 'Révoquée' : 'Active';
  }

  async function loadApiKeys() {
    const tbody = el('api-keys-table-body');
    if (currentUser && !currentUser.permissions.includes('manage_settings')) return;
    try {
      const res = await apiFetch('/api/v1/api-keys');
      cachedApiKeys = await res.json();
      renderApiKeysTable();
      if (typeof renderSettingsOverview === 'function') renderSettingsOverview();
    } catch {
      if (tbody) tbody.innerHTML = emptyStateRow(6, 'Impossible de récupérer les clés d\'API.', true);
    }
  }

  function renderApiKeysTable() {
    const tbody = el('api-keys-table-body');
    if (!tbody) return;
    if (cachedApiKeys.length === 0) {
      tbody.innerHTML = emptyStateRow(6, 'Aucune clé d\'API. Créez-en une pour raccorder un worker.', false);
      return;
    }
    tbody.innerHTML = cachedApiKeys.map(key => {
      const revoked = Boolean(key.revokedAt);
      return `<tr class="registre-row" data-key-id="${escapeHtml(key.id)}">
          <td class="text-strong">${escapeHtml(key.name)}</td>
          <td>${API_KEY_ROLE_LABELS[key.role] || escapeHtml(key.role)}</td>
          <td>${formatDate(key.createdAt)}</td>
          <td>${key.lastUsedAt ? formatDate(key.lastUsedAt) : 'Jamais'}</td>
          <td>${renderStatusPill(apiKeyState(key), revoked ? 'offline' : 'online')}</td>
          <td class="cell-end"><button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-key-id="${escapeHtml(key.id)}">Ouvrir</button></td>
      </tr>`;
    }).join('');
    tbody.querySelectorAll('.registre-open-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        markRegistreRow(tbody, btn.closest('tr'));
        showApiKeyDetail(btn.getAttribute('data-key-id'));
      });
    });
  }

  function showApiKeyDetail(id) {
    const key = cachedApiKeys.find(item => item.id === id);
    if (!key) return;
    selectedApiKeyId = id;
    const setText = (suffix, text) => {
      const node = el(`api-key-detail-${suffix}`);
      if (node) node.textContent = text;
    };
    setText('id', key.id);
    setText('name', key.name);
    setText('role', API_KEY_ROLE_LABELS[key.role] || key.role);
    setText('created', formatDate(key.createdAt));
    setText('used', key.lastUsedAt ? formatDate(key.lastUsedAt) : 'Jamais');
    setText('state', apiKeyState(key));
    setHidden(['api-key-form', 'api-key-created', 'api-key-create-btn'], true);
    setHidden(['api-key-detail'], false);
    showRegistreFact('api-keys-fact', key.name, [API_KEY_ROLE_LABELS[key.role] || key.role, apiKeyState(key)]);
    setHidden(['api-key-revoke-btn'], Boolean(key.revokedAt));
  }

  function showApiKeyForm() {
    selectedApiKeyId = null;
    el('api-key-name').value = '';
    setHidden(['api-key-detail', 'api-key-created', 'api-key-revoke-btn'], true);
    setHidden(['api-key-form', 'api-key-create-btn'], false);
    showRegistreFact('api-keys-fact', 'Nouvelle clé d\'API', ['Affichée une seule fois']);
    el('api-key-name').focus();
  }

  async function createApiKey() {
    const name = el('api-key-name').value.trim();
    if (!name) {
      showNotice('Donnez un nom à la clé.', 'error');
      return;
    }
    try {
      const res = await apiFetch('/api/v1/api-keys', {
        method: 'POST',
        body: JSON.stringify({ name, role: el('api-key-role').value })
      });
      const created = await res.json();
      el('api-key-created-value').textContent = created.key;
      setHidden(['api-key-form', 'api-key-create-btn'], true);
      setHidden(['api-key-created'], false);
      await loadApiKeys();
    } catch {
      showNotice('Création de la clé impossible.', 'error');
    }
  }

  async function revokeApiKey() {
    if (!selectedApiKeyId || !confirm('Révoquer cette clé ? Les machines qui l\'utilisent perdront l\'accès.')) return;
    try {
      await apiFetch(`/api/v1/api-keys/${encodeURIComponent(selectedApiKeyId)}`, { method: 'DELETE' });
      showNotice('Clé révoquée.', 'success');
      closeInspectionDrawer(el('api-keys-fact'));
      await loadApiKeys();
    } catch {
      showNotice('Révocation impossible.', 'error');
    }
  }

  function copyCreatedApiKey() {
    return copyWithFeedback(el('api-key-copy-btn'), el('api-key-created-value').textContent, 'Clé copiée.');
  }

  function setupApiKeys() {
    const bind = (id, handler) => {
      const node = el(id);
      if (node) node.addEventListener('click', handler);
    };
    bind('api-key-new-btn', showApiKeyForm);
    bind('api-key-create-btn', createApiKey);
    bind('api-key-revoke-btn', revokeApiKey);
    bind('api-key-copy-btn', copyCreatedApiKey);
  }
"###;

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn exposes_key_management_functions() {
        for name in [
            "function loadApiKeys",
            "function createApiKey",
            "function revokeApiKey",
            "function setupApiKeys",
        ] {
            assert!(SCRIPT.contains(name), "{name} manquant");
        }
        assert!(SCRIPT.contains("/api/v1/api-keys"));
    }
}
