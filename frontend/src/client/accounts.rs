//! Comptes : registre, création, changement de rôle et désactivation (ADR 0011).

pub const SCRIPT: &str = r###"
// --- Comptes ---
  let cachedAccounts = [];
  const ACCOUNT_ROLE_CHOICES = ['viewer', 'admin'];
  const ACCOUNT_ERROR_MESSAGES = {
    lease_expired_or_conflict: 'Refusé : cet identifiant existe déjà, ou il doit rester au moins un administrateur actif.',
    invalid_input: 'Vérifiez les champs : identifiant de 3 à 40 caractères, mot de passe d\'au moins 8 caractères.',
    forbidden: 'Action réservée aux administrateurs.'
  };

  function accountErrorMessage(error) {
    return ACCOUNT_ERROR_MESSAGES[error.message] || 'Opération impossible. Réessayez.';
  }

  async function loadAccounts() {
    const tbody = el('accounts-table-body');
    if (currentUser && !currentUser.permissions.includes('manage_accounts')) return;
    try {
      const res = await apiFetch('/api/v1/accounts');
      cachedAccounts = await res.json();
      renderAccountsTable();
      if (typeof renderSettingsOverview === 'function') renderSettingsOverview();
    } catch {
      if (tbody) tbody.innerHTML = emptyStateRow(4, 'Impossible de récupérer les comptes.', true);
    }
  }

  function accountRoleSelect(account) {
    const options = ACCOUNT_ROLE_CHOICES.map(role =>
      `<option value="${role}"${role === account.role ? ' selected' : ''}>${ROLE_LABELS[role]}</option>`
    ).join('');
    return `<select class="form-select" data-account-role="${escapeHtml(account.id)}" aria-label="Rôle de ${escapeHtml(account.username)}">${options}</select>`;
  }

  function renderAccountsTable() {
    const tbody = el('accounts-table-body');
    if (!tbody) return;
    if (cachedAccounts.length === 0) {
      tbody.innerHTML = emptyStateRow(4, 'Aucun compte.', false);
      return;
    }
    tbody.innerHTML = cachedAccounts.map(account => `<tr class="registre-row">
        <td class="text-strong">${escapeHtml(account.username)}</td>
        <td>${accountRoleSelect(account)}</td>
        <td>${renderStatusPill(account.disabled ? 'Désactivé' : 'Actif', account.disabled ? 'offline' : 'online')}</td>
        <td class="cell-end"><button type="button" class="btn btn-secondary btn-sm" data-account-toggle="${escapeHtml(account.id)}">${account.disabled ? 'Réactiver' : 'Désactiver'}</button></td>
    </tr>`).join('');
    tbody.querySelectorAll('[data-account-role]').forEach(select => {
      select.addEventListener('change', () => updateAccount(select.getAttribute('data-account-role'), { role: select.value }));
    });
    tbody.querySelectorAll('[data-account-toggle]').forEach(button => {
      button.addEventListener('click', () => {
        const account = cachedAccounts.find(item => item.id === button.getAttribute('data-account-toggle'));
        if (account) updateAccount(account.id, { disabled: !account.disabled });
      });
    });
  }

  async function updateAccount(id, patch) {
    try {
      await apiFetch(`/api/v1/accounts/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(patch) });
      showNotice('Compte mis à jour. Ses sessions sont fermées.', 'success');
    } catch (error) {
      showNotice(accountErrorMessage(error), 'error');
    }
    await loadAccounts();
  }

  async function createAccount(event) {
    event.preventDefault();
    try {
      await apiFetch('/api/v1/accounts', {
        method: 'POST',
        body: JSON.stringify({
          username: el('account-create-username').value,
          password: el('account-create-password').value,
          role: el('account-create-role').value
        })
      });
      showNotice('Compte créé.', 'success');
      el('account-create-username').value = '';
      el('account-create-password').value = '';
      await loadAccounts();
    } catch (error) {
      showNotice(accountErrorMessage(error), 'error');
    }
  }

  function setupAccounts() {
    const form = el('account-create-form');
    if (form) form.addEventListener('submit', createAccount);
  }
"###;

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn test_script_exposes_the_account_functions_and_routes() {
        for name in [
            "function loadAccounts",
            "function createAccount",
            "function updateAccount",
            "function setupAccounts",
        ] {
            assert!(SCRIPT.contains(name), "{name}");
        }
        assert!(SCRIPT.contains("/api/v1/accounts"));
    }

    #[test]
    fn test_only_human_roles_can_be_chosen() {
        assert!(SCRIPT.contains("['viewer', 'admin']"));
        assert!(!SCRIPT.contains("'worker'"));
    }
}
