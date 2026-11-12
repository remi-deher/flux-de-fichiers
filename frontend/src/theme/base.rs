//! Styles de base et mise en page du shell applicatif en Rust typé.

pub const BASE_CSS: &str = r#"
/* --- Skip Link (Accessibilité Clavier W3C) --- */
.skip-link {
  position: absolute;
  top: -100px;
  left: 1rem;
  background: var(--primary);
  color: var(--primary-contrast);
  padding: 0.5rem 1rem;
  border-radius: var(--radius-sm);
  z-index: 9999;
  text-decoration: none;
  font-weight: 600;
  transition: top 0.2s ease;
}

.skip-link:focus {
  top: 1rem;
}

/* --- Structure globale du shell (Sidebar Rail + Zone de contenu) --- */
.application-shell {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  background-color: var(--bg-app);
  color: var(--text-main);
  box-sizing: border-box;
}

.application-navigation-layout {
  display: flex;
  flex: 1;
  min-height: 100vh;
}

/* --- Barre latérale de navigation (Navigation Rail inspirée d'Encoding Central) --- */
.application-sidebar {
  width: 240px;
  background-color: var(--bg-card);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  transition: width 0.2s ease-in-out;
  position: sticky;
  top: 0;
  height: 100vh;
  overflow-y: auto;
  z-index: 50;
  box-sizing: border-box;
  padding-top: var(--sat);
  padding-bottom: var(--sab);
  padding-left: var(--sal);
}

.application-sidebar.is-collapsed {
  width: 76px;
}

.application-sidebar.is-collapsed .brand-info,
.application-sidebar.is-collapsed .tab-btn-label,
.application-sidebar.is-collapsed .nav-group-label,
.application-sidebar.is-collapsed #theme-toggle-label,
.application-sidebar.is-collapsed #refresh-all-btn,
.application-sidebar.is-collapsed .collapse-label {
  display: none;
}

.application-sidebar.is-collapsed .tab-btn {
  justify-content: center;
  padding: 0.75rem 0.5rem;
}

.application-sidebar.is-collapsed .brand {
  justify-content: center;
}

.application-sidebar.is-collapsed .sidebar-footer {
  padding: 0.75rem 0.5rem;
  align-items: center;
}

.application-sidebar.is-collapsed .sidebar-collapse-btn {
  width: 40px;
  height: 40px;
  padding: 0;
  margin: 0 auto;
  border-radius: var(--radius-sm);
  justify-content: center;
}

.application-sidebar.is-collapsed .sidebar-collapse-btn .collapse-icon {
  transform: rotate(180deg);
}

.application-sidebar.is-collapsed .sidebar-footer-tools {
  width: 100%;
  justify-content: center;
}

.application-sidebar.is-collapsed #theme-toggle-btn {
  width: 40px;
  height: 40px;
  padding: 0;
  justify-content: center;
  min-width: 40px;
}

/* En-tête de la marque */
.sidebar-header {
  padding: 1.25rem 1rem;
  border-bottom: 1px solid var(--border-color);
}

.brand {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  text-decoration: none;
  color: var(--text-main);
}

.brand-mark {
  width: 38px;
  height: 38px;
  border-radius: var(--radius-md);
  background: var(--primary);
  color: var(--primary-contrast);
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 800;
  font-size: 0.8125rem;
  letter-spacing: 0.5px;
  box-shadow: var(--shadow-sm);
  flex-shrink: 0;
}

.brand-info {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.brand-name {
  font-weight: 700;
  font-size: 1rem;
  white-space: nowrap;
}

.brand-version {
  font-size: 0.75rem;
  color: var(--text-dim);
}

/* Groupes et boutons de navigation */
.tab-navigation {
  flex: 1;
  padding: 1rem 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.tab-list {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.nav-group-label {
  font-size: 0.6875rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-dim);
  padding: 0.75rem 0.5rem 0.25rem 0.5rem;
  user-select: none;
}

.tab-btn {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  width: 100%;
  min-height: var(--touch-target-compact);
  padding: 0.625rem 0.75rem;
  border-radius: var(--radius-sm);
  background: transparent;
  border: 1px solid transparent;
  color: var(--text-dim);
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  text-align: left;
  user-select: none;
  touch-action: manipulation;
  transition: all 0.15s ease-in-out;
}

.tab-btn:hover {
  background-color: var(--bg-subtle);
  color: var(--text-main);
}

.tab-btn:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 1px;
}

.tab-btn[aria-selected="true"] {
  background-color: var(--primary-bg);
  color: var(--primary);
  font-weight: 600;
  border-color: var(--primary);
}

.tab-role-icon {
  font-size: 1.125rem;
  flex-shrink: 0;
}

.tab-btn-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Pied de la barre latérale */
.sidebar-footer {
  padding: 0.875rem 0.75rem;
  border-top: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.sidebar-collapse-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.45rem 0.6rem;
  border-radius: var(--radius-sm);
  background: transparent;
  border: 1px solid var(--border-color);
  color: var(--text-dim);
  font-size: 0.75rem;
  cursor: pointer;
  transition: all 0.2s ease-in-out;
}

.sidebar-collapse-btn:hover {
  background-color: var(--bg-subtle);
  color: var(--text-main);
  border-color: var(--primary);
}

.collapse-icon {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.sidebar-footer-tools {
  display: flex;
  gap: 0.5rem;
}

.sidebar-footer-tools button {
  flex: 1;
}

/* --- Zone de transition & contenu principal --- */
.page-transition-region {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background-color: var(--bg-app);
}

/* En-tête applicatif supérieur avec Glassmorphism & Positionnement fixe stable */
.application-header {
  min-height: 60px;
  height: 60px;
  background-color: rgba(22, 27, 34, 0.85);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-bottom: 1px solid var(--border-color);
  position: sticky;
  top: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  padding: var(--sat) calc(1.5rem + var(--sar)) 0 calc(1.5rem + var(--sal));
  box-sizing: border-box;
}

[data-theme="light"] .application-header {
  background-color: rgba(255, 255, 255, 0.85);
}

.header-breadcrumb {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: calc(50% - 250px);
}

/* Style des icônes vectorielles Lucide */
.lucide-icon {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
}

/* Barre de recherche contextuelle du header (strictement centrée en position fixe absolue) */
.header-search-wrapper {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  width: min(100% - 360px, 460px);
  max-width: 460px;
  margin: 0;
  z-index: 3;
}

.header-search-box {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.35rem 0.65rem;
  background-color: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  color: var(--text-dim);
  transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  box-sizing: border-box;
}

.header-search-box:focus-within {
  border-color: var(--primary);
  background-color: var(--bg-card);
  box-shadow: 0 0 0 3px rgba(31, 111, 235, 0.15);
  color: var(--text-main);
}

.header-search-box .search-icon {
  flex-shrink: 0;
  color: var(--text-muted);
}

.header-search-input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  outline: none;
  color: var(--text-main);
  font-size: 0.8125rem;
  font-family: inherit;
  padding: 0.1rem 0;
}

.header-search-input::placeholder {
  color: var(--text-muted);
  font-style: italic;
  transition: color 0.15s ease-in-out;
}

.header-search-input::-webkit-search-cancel-button,
.header-search-input::-webkit-search-decoration {
  -webkit-appearance: none;
  appearance: none;
}

.header-search-clear-btn {
  background: transparent;
  border: none;
  padding: 0.1rem;
  cursor: pointer;
  color: var(--text-muted);
  border-radius: var(--radius-sm);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  transition: color 0.15s ease-in-out;
}

.header-search-clear-btn:hover {
  color: var(--text-main);
}

.search-kbd {
  display: inline-flex;
  align-items: center;
  padding: 0.1rem 0.35rem;
  background-color: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--text-muted);
  user-select: none;
  flex-shrink: 0;
}

/* Actions d'en-tête (verrouillées en position fixe à droite) */
.header-actions {
  position: absolute;
  right: calc(1.5rem + var(--sar));
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  gap: 0.65rem;
  z-index: 2;
}

.header-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: var(--radius-sm);
  background: transparent;
  border: 1px solid transparent;
  color: var(--text-dim);
  cursor: pointer;
  transition: all 0.15s ease-in-out;
  user-select: none;
  touch-action: manipulation;
}

.header-icon-btn:hover {
  background-color: var(--bg-subtle);
  color: var(--text-main);
  border-color: var(--border-color);
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.is-spinning svg,
.is-spinning .lucide-icon {
  animation: spin 0.8s linear infinite;
}



.header-breadcrumb {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.875rem;
}

.breadcrumb-item {
  color: var(--text-dim);
}

.breadcrumb-separator {
  color: var(--text-dim);
}

.breadcrumb-title {
  color: var(--text-main);
  font-weight: 600;
}

.header-status {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

/* Zone de contenu principale */
.main-content {
  flex: 1;
  padding: 1.5rem;
  max-width: 1400px;
  width: 100%;
  margin: 0 auto;
  box-sizing: border-box;
}

/* --- Hub d'accueil : Vue d'ensemble & Statistiques --- */
.overview-hero-card {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1.5rem;
  padding: 1.5rem 1.75rem;
  margin-bottom: 1.5rem;
  background: linear-gradient(135deg, rgba(22, 27, 34, 0.95) 0%, rgba(31, 111, 235, 0.08) 100%);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  box-sizing: border-box;
}

[data-theme="light"] .overview-hero-card {
  background: linear-gradient(135deg, #ffffff 0%, rgba(31, 111, 235, 0.06) 100%);
}

.hero-content {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.hero-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  width: fit-content;
  padding: 0.2rem 0.65rem;
  border-radius: 20px;
  background-color: rgba(46, 160, 67, 0.15);
  color: #3fb950;
  font-size: 0.75rem;
  font-weight: 600;
  border: 1px solid rgba(46, 160, 67, 0.3);
}

.hero-title {
  font-size: 1.4rem;
  font-weight: 700;
  color: var(--text-main);
  margin: 0.15rem 0;
  letter-spacing: -0.02em;
}

.hero-subtitle {
  font-size: 0.875rem;
  color: var(--text-dim);
  margin: 0;
  max-width: 720px;
  line-height: 1.45;
}

.hero-actions {
  flex-shrink: 0;
}

/* Grille de métriques et KPIs statistiques */
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
  gap: 1rem;
  margin-bottom: 1.5rem;
}

/* Grille bi-colonne pour la vue d'ensemble */
.overview-grid-2col {
  display: grid;
  grid-template-columns: 1.25fr 1fr;
  gap: 1.25rem;
  margin-bottom: 1.5rem;
}

@media (max-width: 960px) {
  .overview-grid-2col {
    grid-template-columns: 1fr;
  }
}

/* Distribution des formats de médias & Stockage */
.media-insights-wrapper {
  display: flex;
  flex-direction: column;
}

.insights-section-title {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 0.8125rem;
  color: var(--text-dim);
  margin-bottom: 0.35rem;
}

.segmented-distribution-bar {
  height: 12px;
  border-radius: 6px;
  background-color: var(--bg-subtle);
  display: flex;
  overflow: hidden;
  margin: 0.5rem 0 0.85rem 0;
  border: 1px solid var(--border-color);
}

.segment-fill {
  height: 100%;
  transition: width 0.3s ease;
}

.segment-fill.mkv { background-color: var(--primary); }
.segment-fill.mp4 { background-color: #a371f7; }
.segment-fill.other { background-color: #e3b341; }
.segment-fill.default-segment { background-color: var(--border-color); }

.distribution-legend {
  display: flex;
  gap: 1.25rem;
  flex-wrap: wrap;
  font-size: 0.8125rem;
  color: var(--text-dim);
  margin-bottom: 1.25rem;
}

.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
}

.legend-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.legend-dot.mkv { background-color: var(--primary); }
.legend-dot.mp4 { background-color: #a371f7; }
.legend-dot.other { background-color: #e3b341; }

.storage-breakdown {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  padding-top: 0.85rem;
  border-top: 1px solid var(--border-color);
}

.storage-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.8125rem;
}

.storage-label {
  color: var(--text-dim);
}

.storage-val {
  font-weight: 600;
  color: var(--text-main);
  font-family: var(--font-mono);
  font-size: 0.75rem;
}

/* Panneaux d'onglets */
.tab-panel[hidden] {
  display: none !important;
}

/* Pied de page du shell */
.shell-footer {
  margin-top: auto;
  text-align: center;
  font-size: 0.8125rem;
  color: var(--text-dim);
  border-top: 1px solid var(--border-color);
  padding: 1.25rem 1rem;
  background-color: var(--bg-card);
}

/* --- Gabarit partagé de sous-navigation horizontale (ADR 0020) --- */
.page-subnav {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  margin-bottom: 1.25rem;
  padding: 0.5rem 0.75rem;
  box-shadow: var(--shadow-sm);
}

.subnav-container {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.subnav-info {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--text-dim);
}

.subnav-info:empty {
  display: none;
}

.subnav-info-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.25rem 0.65rem;
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-main);
  white-space: nowrap;
}

.subnav-tabs {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  overflow-x: auto;
  scrollbar-width: thin;
  -webkit-overflow-scrolling: touch;
}

.subnav-tab {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.45rem 0.85rem;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--text-dim);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.15s ease-in-out;
  white-space: nowrap;
}

.subnav-tab:hover {
  color: var(--text-main);
  background: var(--bg-subtle);
}

.subnav-tab:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 1px;
}

.subnav-tab.active {
  color: var(--primary);
  background: var(--primary-bg);
  border-color: var(--primary);
  font-weight: 600;
}

.subnav-tab-icon {
  font-size: 1rem;
}

.subnav-tab-badge {
  display: inline-block;
  padding: 0.1rem 0.4rem;
  font-size: 0.75rem;
  border-radius: 9999px;
  background: var(--bg-subtle);
  color: var(--text-dim);
  border: 1px solid var(--border-color);
}

.subnav-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-left: auto;
  flex-wrap: wrap;
}

.subnav-actions:empty {
  display: none;
}

/* --- Adaptations Responsives Mobile (< 768px) --- */
@media (max-width: 768px) {
  html, body {
    overflow-x: hidden;
    max-width: 100vw;
  }

  .subnav-container {
    flex-direction: column;
    align-items: stretch;
    gap: 0.5rem;
  }

  .subnav-tabs {
    width: 100%;
    max-width: 100%;
    overflow-x: auto;
    scrollbar-width: thin;
    -webkit-overflow-scrolling: touch;
  }

  .subnav-info {
    justify-content: flex-start;
  }

  .subnav-actions {
    margin-left: 0;
    justify-content: flex-start;
    flex-wrap: wrap;
  }

  .application-shell {
    overflow-x: hidden;
    max-width: 100vw;
  }

  .application-navigation-layout {
    flex-direction: column;
    overflow-x: hidden;
    max-width: 100vw;
    min-width: 0;
  }

  .application-sidebar {
    width: 100%;
    max-width: 100vw;
    min-width: 0;
    flex: none;
    border-right: none;
    border-bottom: 1px solid var(--border-color);
    overflow-x: hidden;
  }

  .sidebar-header {
    padding: 0.75rem 1rem;
    max-width: 100vw;
    box-sizing: border-box;
  }

  .tab-navigation {
    padding: 0.5rem;
    max-width: 100vw;
    min-width: 0;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    box-sizing: border-box;
  }

  .tab-list {
    flex-direction: row;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    max-width: 100%;
    width: 100%;
    min-width: 0;
    padding-bottom: 4px;
    scrollbar-width: thin;
    box-sizing: border-box;
  }

  .nav-group-label {
    display: none;
  }

  .tab-btn {
    width: auto;
    flex: 0 0 auto;
    white-space: nowrap;
    padding: 0.5rem 0.75rem;
    font-size: 0.8125rem;
  }

  .sidebar-footer {
    display: none;
  }

  .page-transition-region {
    max-width: 100vw;
    min-width: 0;
  }

  .application-header {
    padding: 0 1rem;
    max-width: 100vw;
    box-sizing: border-box;
    position: relative;
    justify-content: space-between;
  }

  .header-breadcrumb {
    display: none;
  }

  .header-search-wrapper {
    position: static;
    transform: none;
    flex: 1;
    width: auto;
    max-width: none;
    margin-right: 0.5rem;
  }

  .header-actions {
    position: static;
    transform: none;
  }

  .main-content {
    padding: 1rem;
    max-width: 100vw;
    box-sizing: border-box;
  }
}
"#;
