//! Gestion des onglets et navigation ARIA.

pub const SCRIPT: &str = r###"
// --- Gestion des onglets accessibles ---
  const TABS = ['dashboard', 'files', 'flows', 'jobs', 'workers', 'plugins', 'settings', 'updates'];

  const TAB_TITLES = {
    dashboard: 'Vue d\'ensemble & Statistiques',
    files: 'Bibliothèques & Médias',
    flows: 'Concepteur de flux (Flows)',
    jobs: 'Activité & Diagnostics',
    workers: 'Infrastructure & Nœuds',
    plugins: 'Plugins & Outils média',
    settings: 'Paramètres d\'administration',
    updates: 'Mises à jour & Conteneur',
  };

  const SEARCH_CONTEXTS = {
    dashboard: {
      placeholder: 'Rechercher dans la vue d\'ensemble…',
      label: 'Filtrer les indicateurs, statistiques et raccourcis d\'accueil',
    },
    files: {
      placeholder: 'Filtrer les fichiers et bibliothèques…',
      label: 'Filtrer les fichiers médias et dossiers sources',
    },
    flows: {
      placeholder: 'Filtrer les flux et modèles…',
      label: 'Rechercher parmi les modèles de flux et étapes',
    },
    jobs: {
      placeholder: 'Filtrer les tâches et logs…',
      label: 'Filtrer les tâches enregistrées et diagnostics',
    },
    workers: {
      placeholder: 'Filtrer les nœuds et agents…',
      label: 'Filtrer les serveurs et ressources distribuées',
    },
    plugins: {
      placeholder: 'Filtrer les outils et extensions…',
      label: 'Rechercher un outil MKVToolNix ou dépôt de plugins',
    },
    settings: {
      placeholder: 'Filtrer les paramètres système…',
      label: 'Rechercher une section ou clé de configuration',
    },
    updates: {
      placeholder: 'Filtrer les notes de mise à jour…',
      label: 'Rechercher dans l\'historique des versions',
    },
  };

  function updateSearchContext(tabName) {
    const input = el('global-search-input');
    if (!input) return;
    const ctx = SEARCH_CONTEXTS[tabName] || {
      placeholder: 'Rechercher…',
      label: 'Recherche contextuelle',
    };
    input.placeholder = ctx.placeholder;
    input.setAttribute('aria-label', ctx.label);

    // Réapplique immédiatement le filtrage sur la nouvelle vue active
    applyContextualSearch(input.value);
  }

  function applyContextualSearch(query) {
    const term = (query || '').toLowerCase().trim();
    const clearBtn = el('header-search-clear-btn');
    if (clearBtn) {
      clearBtn.hidden = !term;
    }

    switch (activeTab) {
      case 'files': {
        const filesSearch = el('files-search-input');
        if (filesSearch && filesSearch.value !== query) {
          filesSearch.value = query;
        }
        if (typeof renderFilesTable === 'function') {
          renderFilesTable();
        }
        break;
      }
      case 'jobs': {
        const jobsSearch = el('jobs-search-input');
        if (jobsSearch && jobsSearch.value !== query) {
          jobsSearch.value = query;
        }
        if (typeof renderJobsTable === 'function') {
          renderJobsTable();
        }
        break;
      }
      case 'dashboard': {
        const metrics = document.querySelectorAll('#panel-dashboard .metric-card');
        metrics.forEach(card => {
          const match = !term || card.textContent.toLowerCase().includes(term);
          card.style.display = match ? '' : 'none';
        });
        const actions = document.querySelectorAll('#panel-dashboard button');
        actions.forEach(btn => {
          const match = !term || btn.textContent.toLowerCase().includes(term);
          btn.style.display = match ? '' : 'none';
        });
        break;
      }
      case 'flows': {
        const flowCards = document.querySelectorAll('#panel-flows .flow-card, #panel-flows .catalog-flow-card, #panel-flows .palette-node-item');
        flowCards.forEach(card => {
          const match = !term || card.textContent.toLowerCase().includes(term);
          card.style.display = match ? '' : 'none';
        });
        break;
      }
      case 'workers': {
        const rows = document.querySelectorAll('#workers-table-body tr');
        rows.forEach(tr => {
          const match = !term || tr.textContent.toLowerCase().includes(term);
          tr.style.display = match ? '' : 'none';
        });
        break;
      }
      case 'plugins': {
        const cards = document.querySelectorAll('#panel-plugins .tool-card, #panel-plugins .plugin-card, #panel-plugins .plugin-store-card, #repos-table-body tr');
        cards.forEach(card => {
          const match = !term || card.textContent.toLowerCase().includes(term);
          card.style.display = match ? '' : 'none';
        });
        break;
      }
      case 'settings': {
        const sections = document.querySelectorAll('#panel-settings .section-panel');
        sections.forEach(sec => {
          const match = !term || sec.textContent.toLowerCase().includes(term);
          sec.style.display = match ? '' : 'none';
        });
        break;
      }
      case 'updates': {
        const cards = document.querySelectorAll('#panel-updates .card, #panel-updates .section-panel');
        cards.forEach(card => {
          const match = !term || card.textContent.toLowerCase().includes(term);
          card.style.display = match ? '' : 'none';
        });
        break;
      }
    }
  }

  function switchTab(tabName) {
    if (!TABS.includes(tabName)) return;
    activeTab = tabName;

    const titleEl = el('current-section-title');
    if (titleEl && TAB_TITLES[tabName]) {
      titleEl.textContent = TAB_TITLES[tabName];
    }

    TABS.forEach(name => {
      const btn = el(`tab-btn-${name}`);
      const panel = el(`panel-${name}`);
      const isSelected = name === tabName;

      if (btn) {
        btn.setAttribute('aria-selected', isSelected ? 'true' : 'false');
        btn.setAttribute('tabindex', isSelected ? '0' : '-1');
      }
      if (panel) {
        if (isSelected) {
          panel.removeAttribute('hidden');
        } else {
          panel.setAttribute('hidden', '');
        }
      }
    });

    updateSearchContext(tabName);
  }

  function setupTabs() {
    TABS.forEach((name, index) => {
      const btn = el(`tab-btn-${name}`);
      if (!btn) return;

      btn.addEventListener('click', () => switchTab(name));

      btn.addEventListener('keydown', (e) => {
        let targetIndex = index;
        if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
          e.preventDefault();
          targetIndex = (index + 1) % TABS.length;
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
          e.preventDefault();
          targetIndex = (index - 1 + TABS.length) % TABS.length;
        } else if (e.key === 'Home') {
          e.preventDefault();
          targetIndex = 0;
        } else if (e.key === 'End') {
          e.preventDefault();
          targetIndex = TABS.length - 1;
        } else {
          return;
        }
        const nextBtn = el(`tab-btn-${TABS[targetIndex]}`);
        if (nextBtn) {
          nextBtn.focus();
          switchTab(TABS[targetIndex]);
        }
      });
    });

    const qFiles = el('quick-goto-files-btn');
    if (qFiles) qFiles.addEventListener('click', () => switchTab('files'));

    const qJobs = el('quick-goto-jobs-btn');
    if (qJobs) qJobs.addEventListener('click', () => switchTab('jobs'));

    const qSettings = el('quick-goto-settings-btn');
    if (qSettings) qSettings.addEventListener('click', () => switchTab('settings'));

    const qUpdates = el('quick-goto-updates-btn');
    if (qUpdates) qUpdates.addEventListener('click', () => switchTab('updates'));

    setupSidebar();
    setupSubnavTabs();
  }

  function setupSidebar() {
    const collapseBtn = el('sidebar-collapse-btn');
    const sidebar = document.querySelector('.application-sidebar');
    if (collapseBtn && sidebar) {
      collapseBtn.addEventListener('click', () => {
        const isCollapsed = sidebar.classList.toggle('is-collapsed');
        collapseBtn.setAttribute('aria-expanded', isCollapsed ? 'false' : 'true');
        const tooltip = isCollapsed ? 'Déplier la navigation' : 'Replier la navigation';
        collapseBtn.setAttribute('title', tooltip);
        collapseBtn.setAttribute('aria-label', tooltip);
        const label = collapseBtn.querySelector('.collapse-label');
        if (label) label.textContent = isCollapsed ? 'Déplier' : 'Replier';
      });
    }
  }

  // --- Gestion générique de la sous-navigation (ADR 0020) ---
  function switchSubtab(tabBtn) {
    const parentNav = tabBtn.closest('.page-subnav');
    if (!parentNav) return;
    const targetId = tabBtn.getAttribute('data-subtab');
    if (!targetId) return;

    parentNav.querySelectorAll('.subnav-tab').forEach(b => {
      b.classList.remove('active');
      b.setAttribute('aria-selected', 'false');
      b.setAttribute('tabindex', '-1');
      const otherTargetId = b.getAttribute('data-subtab');
      if (otherTargetId && otherTargetId !== targetId) {
        const otherEl = el(otherTargetId);
        if (otherEl) otherEl.setAttribute('hidden', '');
      }
    });

    tabBtn.classList.add('active');
    tabBtn.setAttribute('aria-selected', 'true');
    tabBtn.setAttribute('tabindex', '0');

    const targetEl = el(targetId);
    if (targetEl) {
      targetEl.removeAttribute('hidden');
    }

    document.dispatchEvent(new CustomEvent('flux:subnav-change', { detail: { targetId, tabBtn } }));
  }

  function setupSubnavTabs() {
    document.querySelectorAll('.subnav-tab').forEach(btn => {
      btn.addEventListener('click', () => switchSubtab(btn));
      btn.addEventListener('keydown', (e) => {
        const parentNav = btn.closest('.page-subnav');
        if (!parentNav) return;
        const tabs = Array.from(parentNav.querySelectorAll('.subnav-tab'));
        const index = tabs.indexOf(btn);
        let targetIndex = index;
        if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
          e.preventDefault();
          targetIndex = (index + 1) % tabs.length;
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
          e.preventDefault();
          targetIndex = (index - 1 + tabs.length) % tabs.length;
        } else if (e.key === 'Home') {
          e.preventDefault();
          targetIndex = 0;
        } else if (e.key === 'End') {
          e.preventDefault();
          targetIndex = tabs.length - 1;
        } else {
          return;
        }
        tabs[targetIndex].focus();
        switchSubtab(tabs[targetIndex]);
      });
    });
  }

  // --- Gestion du Thème (Clair / Sombre / Système) ---
  function applyTheme(theme) {
    if (theme === 'dark') {
      document.documentElement.setAttribute('data-theme', 'dark');
    } else if (theme === 'light') {
      document.documentElement.setAttribute('data-theme', 'light');
    } else {
      document.documentElement.removeAttribute('data-theme');
    }
    const select = el('theme-select');
    if (select) select.value = theme;
    try {
      localStorage.setItem('flux_theme_pref', theme);
    } catch {}
  }

  function setupTheme() {
    let saved = 'system';
    try {
      saved = localStorage.getItem('flux_theme_pref') || 'system';
    } catch {}
    applyTheme(saved);

    const select = el('theme-select');
    if (select) {
      select.addEventListener('change', (e) => applyTheme(e.target.value));
    }

    function toggleThemeCycle() {
      const current = document.documentElement.getAttribute('data-theme');
      if (current === 'dark') {
        applyTheme('light');
      } else if (current === 'light') {
        applyTheme('system');
      } else {
        applyTheme('dark');
      }
    }

    const toggleBtn = el('theme-toggle-btn');
    if (toggleBtn) toggleBtn.addEventListener('click', toggleThemeCycle);


    if (window.matchMedia) {
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
        try {
          if (!localStorage.getItem('flux_theme_pref') || localStorage.getItem('flux_theme_pref') === 'system') {
            applyTheme('system');
          }
        } catch {}
      });
    }
  }


"###;
