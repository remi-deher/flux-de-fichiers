//! Page Paramètres : état système, vue d'ensemble et points à surveiller (ADR 0011).

pub const SCRIPT: &str = r###"
// --- Paramètres : état système, vue d'ensemble, clé de chiffrement ---
  let cachedSystemStatus = null;
  const BYTE_UNITS = ['o', 'Ko', 'Mo', 'Go', 'To'];
  const BYTES_PER_UNIT = 1024;
  const KEY_SOURCE_LABELS = { local: 'Fichier local (/config/secret_key)', external: 'Fichier fourni (SECRET_KEY_FILE)' };
  // Onglets de la sous-barre ouverts depuis la vue d'ensemble.
  const SETTINGS_TAB_USERS = 'btn-subtab-settings-users';
  const SETTINGS_TAB_KEYS = 'btn-subtab-settings-keys';
  const SETTINGS_TAB_NETWORK = 'btn-subtab-settings-network';
  const SETTINGS_TAB_STORAGE = 'btn-subtab-settings-storage';

  // 4 300 000 octets s'écrivent « 4,1 Mo » : la plus grande unité qui garde un nombre à un chiffre ou plus.
  function formatBytes(bytes) {
    if (typeof bytes !== 'number' || bytes < 0) return '—';
    let value = bytes;
    let unit = 0;
    while (value >= BYTES_PER_UNIT && unit < BYTE_UNITS.length - 1) {
      value /= BYTES_PER_UNIT;
      unit += 1;
    }
    const text = unit === 0 ? String(Math.round(value)) : value.toFixed(1).replace('.', ',');
    return `${text} ${BYTE_UNITS[unit]}`;
  }

  async function loadSystemStatus() {
    if (currentUser && !currentUser.permissions.includes('manage_settings')) return;
    try {
      const res = await apiFetch('/api/v1/system/status');
      cachedSystemStatus = await res.json();
      renderSystemStatus();
    } catch {
      cachedSystemStatus = null;
    }
  }

  function renderSystemStatus() {
    const status = cachedSystemStatus;
    if (!status) return;
    const setText = (id, text) => {
      const node = el(id);
      if (node) node.textContent = text;
    };
    setText('security-key-id', status.keyId);
    setText('security-key-source', KEY_SOURCE_LABELS[status.keySource] || status.keySource);
    setText('settings-overview-version', status.version);
    renderSettingsOverview();
  }

  function setTile(id, value, subtitle) {
    const valueNode = el(id);
    if (!valueNode) return;
    valueNode.textContent = value;
    const subtitleNode = valueNode.closest('.metric-card')?.querySelector('.metric-subtitle');
    if (subtitleNode && subtitle) subtitleNode.textContent = subtitle;
  }

  function onlineNodes() {
    return cachedWorkers.filter(worker => isWorkerOnline(worker));
  }

  // Ce qui mérite l'attention de l'administrateur, du plus urgent au plus informatif.
  function settingsAlerts() {
    const alerts = [];
    if (location.protocol === 'http:') {
      alerts.push({ tone: 'warning', text: 'Accès en HTTP simple : les mots de passe circulent en clair. Placez la centrale derrière un proxy TLS.', opens: SETTINGS_TAB_NETWORK });
    }
    const offline = cachedWorkers.filter(worker => !isLocalCoordinator(worker) && !isWorkerOnline(worker));
    if (offline.length > 0) {
      alerts.push({ tone: 'warning', text: `Nœud hors ligne : ${offline.map(worker => worker.name || worker.id).join(', ')}`, opens: SETTINGS_TAB_STORAGE });
    }
    const admins = cachedAccounts.filter(account => account.role === 'admin' && !account.disabled);
    if (cachedAccounts.length > 0 && admins.length === 1) {
      alerts.push({ tone: 'warning', text: 'Un seul administrateur actif : créez-en un second pour ne pas perdre l\'accès.', opens: SETTINGS_TAB_USERS });
    }
    if (cachedSystemStatus) {
      alerts.push({ tone: 'ok', text: `Chiffrement des secrets : clé ${cachedSystemStatus.keyId} active`, opens: SETTINGS_TAB_KEYS });
    }
    return alerts;
  }

  function renderSettingsOverview() {
    const nodes = cachedWorkers;
    setTile('settings-tile-nodes', `${onlineNodes().length} / ${nodes.length}`);
    const activeAccounts = cachedAccounts.filter(account => !account.disabled);
    const viewers = activeAccounts.filter(account => account.role === 'viewer').length;
    setTile('settings-tile-accounts', String(activeAccounts.length), viewers === 1 ? '1 lecteur' : `${viewers} lecteurs`);
    setTile('settings-tile-keys', String(cachedApiKeys.filter(key => !key.revokedAt).length));
    setTile('settings-tile-database', cachedSystemStatus ? formatBytes(cachedSystemStatus.databaseBytes) : '—');
    const host = el('settings-alerts');
    if (!host) return;
    const alerts = settingsAlerts();
    host.innerHTML = alerts.length === 0
      ? '<p class="form-help">Rien à signaler.</p>'
      : alerts.map(alert => `<div class="settings-alert" data-tone="${alert.tone === 'warning' ? 'warning' : 'ok'}">
          <span class="settings-alert__dot" aria-hidden="true"></span>
          <span class="settings-alert__text">${escapeHtml(alert.text)}</span>
          <button type="button" class="btn btn-secondary btn-sm" data-subtab-go="${alert.opens}">Ouvrir</button>
        </div>`).join('');
  }

  function setupSettings() {
    // Tuiles et lignes de la vue d'ensemble : un clic ouvre l'onglet de la sous-barre qu'elles désignent.
    document.addEventListener('click', event => {
      const trigger = event.target.closest('[data-subtab-go]');
      if (!trigger) return;
      const tab = el(trigger.getAttribute('data-subtab-go'));
      if (tab) tab.click();
    });
    const copyButton = el('security-rotate-copy-btn');
    if (copyButton) {
      copyButton.addEventListener('click', () => {
        const command = el('security-rotate-cmd').textContent;
        copyWithFeedback(copyButton, command, 'Commande copiée. Remplacez <conteneur> par le nom du conteneur.');
      });
    }
  }
"###;

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn test_script_exposes_the_settings_functions() {
        for name in [
            "function loadSystemStatus",
            "function renderSystemStatus",
            "function renderSettingsOverview",
            "function settingsAlerts",
            "function formatBytes",
            "function setupSettings",
        ] {
            assert!(SCRIPT.contains(name), "{name}");
        }
        assert!(SCRIPT.contains("/api/v1/system/status"));
    }

    #[test]
    fn test_overview_targets_the_tiles_and_tabs_the_page_declares() {
        for id in [
            "settings-tile-nodes",
            "settings-tile-accounts",
            "settings-tile-keys",
            "settings-tile-database",
            "settings-alerts",
            "btn-subtab-settings-users",
            "btn-subtab-settings-keys",
            "btn-subtab-settings-network",
            "btn-subtab-settings-storage",
        ] {
            assert!(SCRIPT.contains(id), "{id}");
        }
    }

    #[test]
    fn test_alerts_cover_http_offline_nodes_and_a_single_admin() {
        assert!(SCRIPT.contains("location.protocol === 'http:'"));
        assert!(SCRIPT.contains("Nœud hors ligne"));
        assert!(SCRIPT.contains("Un seul administrateur actif"));
    }
}
