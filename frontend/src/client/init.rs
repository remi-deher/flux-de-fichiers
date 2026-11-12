//! Initialisation de l'interface et synchronisation.

pub const SCRIPT: &str = r###"
// --- Synchronisation globale ---
  async function refreshAll() {
    const refreshBtn = el('refresh-all-btn');
    const headerRefreshBtn = el('header-refresh-btn');
    if (refreshBtn) {
      refreshBtn.disabled = true;
    }
    if (headerRefreshBtn) {
      headerRefreshBtn.classList.add('is-spinning');
    }

    await checkHealth();
    await checkSession();
    await Promise.allSettled([
      loadFiles(),
      loadJobs(),
      loadWorkers(),
      loadSystemConfig(),
      loadToolsStatus(),
      loadFlows()
    ]);

    if (refreshBtn) {
      refreshBtn.disabled = false;
    }
    if (headerRefreshBtn) {
      headerRefreshBtn.classList.remove('is-spinning');
    }
  }


  // --- Initialisation des écouteurs d'événements ---
  function initListeners() {
    setupTabs();
    setupTheme();

    const refreshBtn = el('refresh-all-btn');
    if (refreshBtn) refreshBtn.addEventListener('click', refreshAll);

    const headerRefreshBtn = el('header-refresh-btn');
    if (headerRefreshBtn) headerRefreshBtn.addEventListener('click', refreshAll);

    const searchTrigger = el('header-search-trigger');
    if (searchTrigger) searchTrigger.addEventListener('click', () => switchTab('files'));

    // Raccourci global clavier Cmd+K / Ctrl+K
    window.addEventListener('keydown', (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        switchTab('files');
      }
    });

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

"###;
