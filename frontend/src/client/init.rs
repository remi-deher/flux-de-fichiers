//! Initialisation de l'interface et synchronisation.

pub const SCRIPT: &str = r###"
// --- Synchronisation globale ---
  async function refreshAll() {
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
  }


  // --- Initialisation des écouteurs d'événements ---
  function initListeners() {
    setupTabs();
    setupTheme();
    updateSearchContext(activeTab);

    // Initialisation de la barre de recherche contextuelle globale
    const globalSearchInput = el('global-search-input');

    if (globalSearchInput) {
      globalSearchInput.addEventListener('input', (e) => {
        applyContextualSearch(e.target.value);
      });

      globalSearchInput.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          globalSearchInput.value = '';
          applyContextualSearch('');
          globalSearchInput.blur();
        }
      });
    }

    // Raccourci global clavier Cmd+K / Ctrl+K : réveille la barre et focus direct sur la recherche
    window.addEventListener('keydown', (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        const topbar = el('app-topbar');
        if (topbar) topbar.classList.remove('is-hidden');
        document.querySelectorAll('.page-subnav').forEach(s => s.classList.remove('is-hidden'));
        if (globalSearchInput) {
          globalSearchInput.focus();
          globalSearchInput.select();
        }
      }
    });

    setupChromeAutoHide();

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

    // Recherche de fichiers (synchronisation bidirectionnelle)
    const filesSearch = el('files-search-input');
    if (filesSearch) {
      filesSearch.addEventListener('input', () => {
        if (globalSearchInput && globalSearchInput.value !== filesSearch.value) {
          globalSearchInput.value = filesSearch.value;
        }
        renderFilesTable();
      });
    }

    // Dialogue confirmation remux
    const closeDialogX = el('confirm-dialog-close-x');
    if (closeDialogX) closeDialogX.addEventListener('click', closeConfirmRemuxDialog);

    const cancelDialogBtn = el('confirm-dialog-cancel-btn');
    if (cancelDialogBtn) cancelDialogBtn.addEventListener('click', closeConfirmRemuxDialog);

    const submitRemuxBtn = el('confirm-dialog-submit-btn');
    if (submitRemuxBtn) submitRemuxBtn.addEventListener('click', submitRemuxJob);

    const launchFlowBtn = el('files-fact-action');
    if (launchFlowBtn) {
      launchFlowBtn.addEventListener('click', () => {
        const title = el('files-fact-title');
        const name = title ? title.textContent : '';
        if (!name || name === 'Sélectionnez une ligne') {
          showNotice('Ouvrez un fichier avant de lancer un flux.', 'info');
          return;
        }
        openConfirmRemuxDialog(name);
      });
    }

    // Recherche de tâches (synchronisation bidirectionnelle)
    const jobsSearch = el('jobs-search-input');
    if (jobsSearch) {
      jobsSearch.addEventListener('input', () => {
        if (globalSearchInput && globalSearchInput.value !== jobsSearch.value) {
          globalSearchInput.value = jobsSearch.value;
        }
        renderJobsTable();
      });
    }

    // Filtres de statut de tâches
    document.querySelectorAll('.filter-chip[data-filter]').forEach(btn => {
      btn.addEventListener('click', () => {
        document.querySelectorAll('.filter-chip[data-filter]').forEach(b => b.setAttribute('aria-pressed', 'false'));
        btn.setAttribute('aria-pressed', 'true');
        activeJobFilter = btn.getAttribute('data-filter') || 'all';
        renderJobsTable();
        updateFilterCount();
      });
    });

    const filterBtn = el('search-filter-btn');
    if (filterBtn) filterBtn.addEventListener('click', toggleFilterSheet);
    const filterClose = el('filter-sheet-close');
    if (filterClose) filterClose.addEventListener('click', closeFilterSheet);
    const filterClear = el('filter-sheet-clear');
    if (filterClear) {
      filterClear.addEventListener('click', () => {
        const all = document.querySelector('#filter-sheet .filter-chip[data-filter="all"]');
        if (all) all.click();
      });
    }
    const searchField = el('global-search-input');
    if (searchField) {
      searchField.addEventListener('focusin', () => {
        if (document.body.hasAttribute('data-filter-sheet')) closeFilterSheet();
      });
    }
    syncSearchCapsule(activeTab);

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

    document.addEventListener('click', (event) => {
      const tab = event.target.closest('.inspection-drawer__tab');
      if (tab) {
        const drawer = tab.closest('.inspection-drawer');
        if (drawer) selectInspectionTab(drawer, tab);
        return;
      }
      const closer = event.target.closest('[data-inspection-close]');
      if (!closer) return;
      const drawer = closer.closest('.inspection-drawer');
      if (drawer) closeInspectionDrawer(drawer);
    });

    // Fermeture avec Échap pour les modales, puis pour le tiroir d'inspection
    window.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        const modalOpen = Boolean(document.querySelector('dialog[open]'));
        closeConfirmRemuxDialog();
        closeJobDetailDialog();
        const wizard = el('flow-wizard-dialog');
        if (wizard && wizard.open) {
          wizard.close();
        }
        if (!modalOpen) {
          document.querySelectorAll('.inspection-drawer.is-open').forEach(closeInspectionDrawer);
        }
      }
    });

    // Synchronisation d'arrière-plan périodique (sonde de santé et relais si déconnexion SSE)
    setInterval(() => {
      checkHealth();
      if (!jobsEventSource || jobsEventSource.readyState !== 1) {
        loadJobs();
        loadWorkers();
      }
    }, 15000);
  }

  // --- Auto-masquage au défilement inspiré de Watchdeck (useChromeAutoHide) ---
  function setupChromeAutoHide() {
    const topbar = el('app-topbar');
    if (!topbar) return;

    const TOP_ZONE_PX = 24;
    const HIDE_AFTER_PX = 96;
    const HIDE_DELTA_PX = 7;
    const SHOW_DELTA_PX = -5;

    let lastScrollY = window.scrollY || 0;

    window.addEventListener('scroll', () => {
      const current = Math.max(0, window.scrollY);
      const delta = current - lastScrollY;
      const subnavs = document.querySelectorAll('.page-subnav');

      // Élévation : renforce l'ombre dès qu'on défile
      if (current >= TOP_ZONE_PX) {
        topbar.classList.add('is-elevated');
      } else {
        topbar.classList.remove('is-elevated');
      }

      // Si le champ de recherche a le focus, on garde la barre visible
      const sheetOpen = document.body.hasAttribute('data-filter-sheet');
      const isFocused = topbar.contains(document.activeElement);
      if (sheetOpen || isFocused || current < TOP_ZONE_PX || delta < SHOW_DELTA_PX) {
        topbar.classList.remove('is-hidden');
        subnavs.forEach(s => s.classList.remove('is-hidden'));
      } else if (delta > HIDE_DELTA_PX && current > HIDE_AFTER_PX) {
        topbar.classList.add('is-hidden');
        subnavs.forEach(s => s.classList.add('is-hidden'));
      }

      lastScrollY = current;
    }, { passive: true });
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
