//! Utilisateurs : registre, tiroir de modification et création (ADR 0011).

pub const SCRIPT: &str = r###"
// --- Utilisateurs ---
  let cachedAccounts = [];
  let selectedAccountId = null;
  // Compte d'un lecteur : il ne connaît que le sien, que le serveur ne lui liste pas.
  const OWN_ACCOUNT_ID = 'self';
  const ACCOUNT_ERROR_MESSAGES = {
    lease_expired_or_conflict: 'Refusé : cet identifiant existe déjà, ou il doit rester au moins un administrateur actif.',
    invalid_input: 'Vérifiez les champs : identifiant de 3 à 40 caractères, mot de passe d\'au moins 8 caractères.',
    forbidden: 'Action réservée aux administrateurs.'
  };

  function accountErrorMessage(error) {
    return ACCOUNT_ERROR_MESSAGES[error.message] || 'Opération impossible. Réessayez.';
  }

  function isAccountAdmin() {
    return Boolean(currentUser) && currentUser.permissions.includes('manage_accounts');
  }

  function isOwnAccount(account) {
    return Boolean(currentUser) && String(account.username).toLowerCase() === String(currentUser.name).toLowerCase();
  }

  function accountState(account) {
    return account.disabled ? 'Désactivé' : 'Actif';
  }

  async function loadAccounts() {
    if (!currentUser) return;
    if (!isAccountAdmin()) {
      cachedAccounts = [{ id: OWN_ACCOUNT_ID, username: currentUser.name, role: currentUser.role, disabled: false }];
      renderAccountsTable();
      return;
    }
    const tbody = el('accounts-table-body');
    try {
      const res = await apiFetch('/api/v1/accounts');
      cachedAccounts = await res.json();
      renderAccountsTable();
      if (typeof renderSettingsOverview === 'function') renderSettingsOverview();
    } catch {
      if (tbody) tbody.innerHTML = emptyStateRow(4, 'Impossible de récupérer les utilisateurs.', true);
    }
  }

  function renderAccountsTable() {
    const tbody = el('accounts-table-body');
    if (!tbody) return;
    if (cachedAccounts.length === 0) {
      tbody.innerHTML = emptyStateRow(4, 'Aucun utilisateur.', false);
      return;
    }
    tbody.innerHTML = cachedAccounts.map(account => `<tr class="registre-row" data-account-id="${escapeHtml(account.id)}">
        <td class="text-strong">${escapeHtml(account.username)}${isOwnAccount(account) ? ' <span class="form-help">(vous)</span>' : ''}</td>
        <td>${escapeHtml(ROLE_LABELS[account.role] || account.role)}</td>
        <td>${renderStatusPill(accountState(account), account.disabled ? 'offline' : 'online')}</td>
        <td class="cell-end"><button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-account-id="${escapeHtml(account.id)}">Modifier</button></td>
    </tr>`).join('');
    tbody.querySelectorAll('.registre-open-btn').forEach(button => {
      button.addEventListener('click', () => {
        markRegistreRow(tbody, button.closest('tr'));
        showAccountDetail(button.getAttribute('data-account-id'));
      });
    });
  }

  // Ouvre le tiroir sur un compte : un administrateur change son rôle et son état, chacun change son mot de passe.
  function showAccountDetail(id) {
    const account = cachedAccounts.find(item => item.id === id);
    if (!account) return;
    selectedAccountId = id;
    const setText = (suffix, text) => {
      const node = el(`account-detail-${suffix}`);
      if (node) node.textContent = text;
    };
    setText('name', account.username);
    setText('role', ROLE_LABELS[account.role] || account.role);
    setText('state', accountState(account));
    el('account-edit-role').value = account.role;
    el('account-toggle-btn').textContent = account.disabled ? 'Réactiver' : 'Désactiver';
    setHidden(['account-create-form', 'account-create-btn'], true);
    setHidden(['account-detail'], false);
    setHidden(['account-admin-controls', 'account-toggle-btn'], !isAccountAdmin());
    setHidden(['account-password-form'], !isOwnAccount(account));
    showRegistreFact('accounts-fact', account.username, [ROLE_LABELS[account.role] || account.role, accountState(account)]);
  }

  function showAccountForm() {
    selectedAccountId = null;
    ['account-create-username', 'account-create-password'].forEach(id => { el(id).value = ''; });
    setHidden(['account-detail', 'account-toggle-btn'], true);
    setHidden(['account-create-form', 'account-create-btn'], false);
    showRegistreFact('accounts-fact', 'Nouvel utilisateur', ['Mot de passe : 8 caractères minimum']);
    el('account-create-username').focus();
  }

  async function updateAccount(id, patch) {
    const account = cachedAccounts.find(item => item.id === id);
    if (!account) return;
    if (isOwnAccount(account) && !confirm('Vous modifiez votre propre compte : votre session sera fermée. Continuer ?')) {
      showAccountDetail(id);
      return;
    }
    try {
      await apiFetch(`/api/v1/accounts/${encodeURIComponent(id)}`, { method: 'PUT', body: JSON.stringify(patch) });
      showNotice('Compte mis à jour. Ses sessions sont fermées.', 'success');
    } catch (error) {
      showNotice(accountErrorMessage(error), 'error');
    }
    await loadAccounts();
    if (cachedAccounts.some(item => item.id === id)) showAccountDetail(id);
  }

  async function createAccount() {
    const username = el('account-create-username').value;
    const password = el('account-create-password').value;
    if (!username.trim() || !password) {
      showNotice('Renseignez l\'identifiant et le mot de passe.', 'error');
      return;
    }
    try {
      await apiFetch('/api/v1/accounts', {
        method: 'POST',
        body: JSON.stringify({ username, password, role: el('account-create-role').value })
      });
      showNotice('Compte créé.', 'success');
      el('account-create-password').value = '';
      closeInspectionDrawer(el('accounts-fact'));
      await loadAccounts();
    } catch (error) {
      showNotice(accountErrorMessage(error), 'error');
    }
  }

  function setupAccounts() {
    const bind = (id, event, handler) => {
      const node = el(id);
      if (node) node.addEventListener(event, handler);
    };
    bind('account-new-btn', 'click', showAccountForm);
    bind('account-create-btn', 'click', createAccount);
    bind('account-create-password', 'keydown', event => {
      if (event.key === 'Enter') createAccount();
    });
    bind('account-edit-role', 'change', event => {
      if (selectedAccountId) updateAccount(selectedAccountId, { role: event.target.value });
    });
    bind('account-toggle-btn', 'click', () => {
      const account = cachedAccounts.find(item => item.id === selectedAccountId);
      if (account) updateAccount(account.id, { disabled: !account.disabled });
    });
  }
"###;

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    #[test]
    fn test_script_exposes_the_account_functions_and_routes() {
        for name in [
            "function loadAccounts",
            "function showAccountDetail",
            "function showAccountForm",
            "function createAccount",
            "function updateAccount",
            "function setupAccounts",
        ] {
            assert!(SCRIPT.contains(name), "{name}");
        }
        assert!(SCRIPT.contains("/api/v1/accounts"));
    }

    #[test]
    fn test_modification_goes_through_the_drawer_not_inline_controls() {
        assert!(SCRIPT.contains("showRegistreFact('accounts-fact'"));
        assert!(SCRIPT.contains(">Modifier</button>"));
        assert!(!SCRIPT.contains("data-account-toggle"));
    }

    #[test]
    fn test_a_viewer_only_sees_their_own_account_and_cannot_edit_roles() {
        assert!(SCRIPT.contains("OWN_ACCOUNT_ID"));
        assert!(SCRIPT.contains(
            "setHidden(['account-admin-controls', 'account-toggle-btn'], !isAccountAdmin())"
        ));
        assert!(SCRIPT.contains("setHidden(['account-password-form'], !isOwnAccount(account))"));
    }

    #[test]
    fn test_only_human_roles_can_be_chosen() {
        assert!(!SCRIPT.contains("'worker'"));
    }
}
