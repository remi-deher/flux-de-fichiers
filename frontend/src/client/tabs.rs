//! Gestion des onglets et navigation ARIA.

pub const SCRIPT: &str = r###"
// --- Gestion des onglets accessibles ---
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
    const matchBadge = el('search-match-count');
    if (clearBtn) {
      clearBtn.hidden = !term;
    }

    let matchCount = 0;
    let totalCount = 0;

    switch (activeTab) {
      case 'files': {
        const filesSearch = el('files-search-input');
        if (filesSearch && filesSearch.value !== query) {
          filesSearch.value = query;
        }
        totalCount = cachedFiles.length;
        matchCount = cachedFiles.filter(f => matchesQuery([f.name], query)).length;
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
        totalCount = cachedJobs.length;
        matchCount = cachedJobs.filter(j => matchesQuery([j.source_name, j.id], query)).length;
        if (typeof renderJobsTable === 'function') {
          renderJobsTable();
        }
        break;
      }
      case 'dashboard': {
        const metrics = document.querySelectorAll('#panel-dashboard .metric-card');
        totalCount = metrics.length;
        metrics.forEach(card => {
          const match = textMatches(card, query);
          if (match) matchCount++;
          setFiltered(card, match);
        });
        const actions = document.querySelectorAll('#panel-dashboard button');
        actions.forEach(btn => setFiltered(btn, textMatches(btn, query)));
        break;
      }
      case 'flows': {
        const flowRows = document.querySelectorAll('#flows-table-body tr[data-flow-id], #panel-flows .palette-node-item');
        totalCount = flowRows.length;
        flowRows.forEach(row => {
          const match = textMatches(row, query);
          if (match) matchCount++;
          setFiltered(row, match);
        });
        break;
      }
      case 'workers': {
        const rows = document.querySelectorAll('#workers-table-body tr');
        totalCount = rows.length;
        rows.forEach(tr => {
          const match = textMatches(tr, query);
          if (match) matchCount++;
          setFiltered(tr, match);
        });
        break;
      }
      case 'plugins': {
        const rows = document.querySelectorAll('#panel-plugins .registre-row');
        totalCount = rows.length;
        rows.forEach(row => {
          const tags = row.getAttribute('data-tags') || '';
          const match = textMatches(row, query) || matchesQuery([tags], query);
          if (match) matchCount++;
          setFiltered(row, match);
        });
        break;
      }
      case 'settings': {
        const sections = document.querySelectorAll('#panel-settings .section-panel');
        totalCount = sections.length;
        sections.forEach(sec => {
          const match = textMatches(sec, query);
          if (match) matchCount++;
          setFiltered(sec, match);
        });
        break;
      }
      case 'updates': {
        const cards = document.querySelectorAll('#panel-updates .card, #panel-updates .section-panel');
        totalCount = cards.length;
        cards.forEach(card => {
          const match = textMatches(card, query);
          if (match) matchCount++;
          setFiltered(card, match);
        });
        break;
      }
    }

    if (matchBadge) {
      if (!term || totalCount === 0) {
        matchBadge.textContent = '';
      } else if (matchCount === totalCount) {
        matchBadge.textContent = `${totalCount}`;
      } else {
        matchBadge.textContent = `${matchCount} sur ${totalCount}`;
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

    // Actualisation silencieuse contextuelle à l'affichage de l'onglet
    if (tabName === 'files') {
      loadFiles();
    } else if (tabName === 'jobs') {
      loadJobs();
    } else if (tabName === 'workers') {
      loadWorkers();
      loadSystemConfig();
      loadToolsStatus();
    } else if (tabName === 'flows') {
      loadFlows();
      const canvas = el('flows-canvas-view');
      const titleEl = el('current-section-title');
      if (canvas && canvas.style.display === 'block' && titleEl) {
        titleEl.textContent = 'Éditeur de flux';
      }
    } else if (tabName === 'plugins') {
      if (typeof loadRepositories === 'function') loadRepositories();
      if (typeof loadStoreCatalog === 'function') loadStoreCatalog();
      if (typeof syncRepositoriesSilently === 'function') syncRepositoriesSilently();
    } else if (tabName === 'dashboard') {
      if (typeof updateOverviewStats === 'function') updateOverviewStats();
    }
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
