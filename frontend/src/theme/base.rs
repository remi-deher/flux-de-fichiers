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
  padding: 0.5rem;
  gap: 0.5rem;
  background-color: var(--bg-app);
  box-sizing: border-box;
}

/* --- Barre latérale de navigation (App Frame style Linear / Raycast) --- */
.application-sidebar {
  width: 244px;
  background-color: var(--bg-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  transition: width 0.22s var(--ease-out);
  position: sticky;
  top: 0.5rem;
  height: calc(100vh - 1rem);
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
.application-sidebar.is-collapsed .tab-btn-badge,
.application-sidebar.is-collapsed .nav-group-label,
.application-sidebar.is-collapsed .collapse-label,
.application-sidebar.is-collapsed .sidebar-cmd-btn__label,
.application-sidebar.is-collapsed .sidebar-cmd-btn__kbd,
.application-sidebar.is-collapsed .sidebar-user-meta {
  display: none;
}

.application-sidebar.is-collapsed .sidebar-cmd-btn {
  justify-content: center;
  padding: 0.5rem;
}

.application-sidebar.is-collapsed .sidebar-user-card {
  flex-direction: column;
  padding: 0.4rem;
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

/* Replié : le thème se règle barre dépliée, la déconnexion reste accessible. */
.application-sidebar.is-collapsed .theme-segmented {
  display: none;
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
  font-size: 0.95rem;
  white-space: nowrap;
}

.brand-meta {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin-top: 0.15rem;
}

.brand-node-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.08rem 0.42rem;
  border-radius: var(--radius-pill);
  background: var(--primary-bg);
  color: var(--primary);
  font-size: 0.65rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.brand-version {
  font-size: 0.72rem;
  color: var(--text-dim);
}

.sidebar-cmd-btn {
  margin-top: 0.75rem;
  width: 100%;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.45rem 0.6rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-dim);
  font-size: 0.8125rem;
  cursor: pointer;
  transition: border-color 0.15s ease, color 0.15s ease;
}

.sidebar-cmd-btn:hover {
  border-color: var(--primary);
  color: var(--text-main);
}

.sidebar-cmd-btn__label {
  flex: 1;
  text-align: left;
  white-space: nowrap;
}

.sidebar-cmd-btn__kbd {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  padding: 0.05rem 0.35rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-muted);
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
  transition: all 0.16s var(--ease-out);
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
  border-color: color-mix(in srgb, var(--primary) 35%, transparent);
  box-shadow: inset 3px 0 0 var(--primary);
}

.tab-role-icon {
  font-size: 1.125rem;
  flex-shrink: 0;
}

.tab-btn-label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tab-btn-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 1.35rem;
  height: 1.2rem;
  padding: 0 0.4rem;
  border-radius: var(--radius-pill);
  font-size: 0.7rem;
  font-weight: 700;
  line-height: 1;
  font-variant-numeric: tabular-nums;
  background: var(--bg-subtle);
  color: var(--text-dim);
  flex-shrink: 0;
}

.tab-btn-badge[data-tone="primary"] {
  background: var(--primary-bg);
  color: var(--primary);
}

.tab-btn-badge[data-tone="success"] {
  background: var(--success-bg);
  color: var(--success);
}

.tab-btn-badge[data-tone="warning"] {
  background: var(--warning-bg);
  color: var(--warning);
}

.tab-btn-badge[data-tone="danger"] {
  background: var(--danger-bg);
  color: var(--danger);
}

/* Pied de la barre latérale */
.sidebar-footer {
  padding: 0.75rem;
  border-top: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.sidebar-user-card {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.45rem 0.55rem;
  border-radius: var(--radius-md);
  background: var(--bg-subtle);
  border: 1px solid var(--border-subtle);
}

.sidebar-user-avatar {
  width: 30px;
  height: 30px;
  border-radius: var(--radius-pill);
  background: var(--primary-bg);
  color: var(--primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 0.75rem;
  font-weight: 700;
  flex-shrink: 0;
}

.sidebar-user-meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.sidebar-user-name {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sidebar-user-role {
  font-size: 0.6875rem;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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

/* Thème : trois boutons exclusifs. */
.theme-segmented {
  display: flex;
  width: 100%;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.theme-segmented__btn {
  flex: 1;
  padding: 0.4rem 0.25rem;
  border: 0;
  background: transparent;
  color: var(--text-dim);
  font-size: 0.75rem;
  cursor: pointer;
}

.theme-segmented__btn:hover {
  color: var(--text-main);
}

.theme-segmented__btn[aria-pressed="true"] {
  background-color: var(--bg-subtle);
  color: var(--text-main);
  font-weight: 600;
}

/* Déconnexion : un bouton discret à côté du thème, toujours accessible. */
.sidebar-footer-tools {
  align-items: center;
}

.sidebar-logout-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 32px;
  height: 32px;
  padding: 0;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

.sidebar-logout-btn:hover {
  border-color: var(--primary);
  color: var(--text-main);
}

/* --- Zone de transition & contenu principal --- */
.page-transition-region {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background-color: var(--bg-elevated);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  overflow: clip;
}

/* --- Barre de commande intégrée dans le panneau principal --- */
.app-topbar {
  position: sticky;
  top: max(0px, var(--sat));
  z-index: 40;
  margin: 0;
  background-color: var(--bg-glass);
  backdrop-filter: blur(16px) saturate(180%);
  -webkit-backdrop-filter: blur(16px) saturate(180%);
  border-bottom: 1px solid var(--border-subtle);
  transition: opacity 0.22s var(--ease-out),
              transform 0.22s var(--ease-out),
              box-shadow 0.2s var(--ease-out),
              border-color 0.2s var(--ease-out);
  box-sizing: border-box;
}

.app-topbar:focus-within {
  border-bottom-color: var(--border-highlight);
}

/* Élévation quand la page est défilée */
.app-topbar.is-elevated {
  border-bottom-color: var(--border-highlight);
  box-shadow: var(--shadow-sm);
}

/* Auto-masquage intelligent au défilement (disparaît vers le haut) */
.app-topbar.is-hidden:not(:focus-within) {
  opacity: 0;
  transform: translateY(calc(-100% - 20px));
  pointer-events: none;
}

.topbar-container {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.5rem 1rem;
  min-height: 52px;
  box-sizing: border-box;
}

.topbar-lead {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  min-width: 0;
}

.topbar-breadcrumb {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.8125rem;
  min-width: 0;
  white-space: nowrap;
}

.topbar-breadcrumb__group {
  color: var(--text-dim);
  font-weight: 500;
}

.topbar-breadcrumb__sep {
  color: var(--text-muted);
}

.topbar-breadcrumb__page {
  color: var(--text-main);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Style des icônes vectorielles Lucide */
.lucide-icon {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
}

/* Capsule de recherche : saisie, actions de la page, bouton Filtres.
   La feuille est un enfant du champ : elle en sort par le bas, à la même largeur. */
/* Bouton du tiroir de navigation : visible seulement sur écran étroit. */
.topbar-menu-btn {
  display: none;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  padding: 0;
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  color: var(--text-main);
  cursor: pointer;
}

.sidebar-scrim {
  display: none;
}

.topbar-search-anchor {
  position: relative;
  flex: 1 1 auto;
  min-width: 0;
  max-width: 720px;
  margin-inline: auto;
}

.topbar-search {
  display: flex;
  align-items: stretch;
  min-height: 36px;
  background: var(--bg-subtle);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  color: var(--text-dim);
  box-sizing: border-box;
  transition: color 0.2s ease, border-color 0.2s ease, background-color 0.2s ease;
}

.topbar-search:focus-within,
body[data-filter-sheet] .topbar-search {
  color: var(--text-main);
  border-color: var(--primary);
  background: var(--bg-card);
}

.search-capsule__query {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  padding: 0 0.65rem;
}

.topbar-search .search-icon {
  flex-shrink: 0;
  color: var(--text-muted);
}

.topbar-search-input {
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

.topbar-search-input::placeholder {
  color: var(--text-muted);
  font-style: italic;
  transition: color 0.15s ease-in-out;
}

.topbar-search-input::-webkit-search-cancel-button,
.topbar-search-input::-webkit-search-decoration {
  -webkit-appearance: none;
  appearance: none;
}

/* Compteur de correspondances réactif (ex: 3 sur 12) */
.search-match-count {
  font-size: 0.6875rem;
  font-weight: 600;
  padding: 0.15rem 0.45rem;
  border-radius: 12px;
  background-color: rgba(88, 166, 255, 0.15);
  color: var(--primary);
  white-space: nowrap;
  user-select: none;
}

.search-match-count:empty {
  display: none;
}

.topbar-search-clear {
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

.topbar-search-clear:hover {
  color: var(--text-main);
}

.filter-sheet {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  z-index: 30;
  padding: 0.75rem;
  background: var(--bg-card);
  border: 1px solid var(--primary);
  border-top: 0;
  border-radius: 0 0 var(--radius-md) var(--radius-md);
  box-shadow: var(--shadow-sm);
}

.filter-sheet__head {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.65rem;
}

.filter-sheet__title {
  font-weight: 700;
  color: var(--text-main);
  margin-right: auto;
}

.filter-sheet__text-btn {
  border: 0;
  background: transparent;
  color: var(--primary);
  font: inherit;
  font-size: 0.8125rem;
  font-weight: 700;
  cursor: pointer;
}

.filter-sheet__label {
  margin: 0 0 0.4rem;
  font-size: 0.75rem;
  font-weight: 700;
  color: var(--text-dim);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.filter-sheet .filter-group {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

/* Actions et statuts de la TopBar alignés à droite dans le flux flex. */
.topbar-actions {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  flex-shrink: 0;
}

.status-pill-link {
  display: inline-flex;
  text-decoration: none;
}

.status-pill-link:hover .status-pill {
  filter: brightness(1.08);
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

.main-content.is-workbench {
  padding: 0;
  max-width: none;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  height: calc(100dvh - 52px - 1rem);
  min-height: 540px;
}

@keyframes pageFadeIn {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.tab-panel {
  animation: pageFadeIn 0.22s var(--ease-out);
}

/* --- Bandeau de KPIs (StatStrip) --- */
.stat-strip {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
  gap: 0.75rem;
  margin-bottom: 1.25rem;
}

.stat-pill {
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 0.35rem;
  padding: 0.7rem 0.95rem;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  text-align: left;
  font: inherit;
  color: inherit;
  transition: border-color 0.16s ease, background-color 0.16s ease, box-shadow 0.16s ease;
}

.stat-pill.is-interactive {
  cursor: pointer;
  user-select: none;
}

.stat-pill.is-interactive:hover {
  border-color: var(--border-highlight);
  background: color-mix(in srgb, var(--primary) 6%, var(--bg-card));
}

.stat-pill.is-interactive.is-active {
  border-color: var(--primary);
  background: color-mix(in srgb, var(--primary) 12%, var(--bg-card));
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--primary) 35%, transparent), var(--shadow-sm);
}

.stat-pill__top {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.5rem;
}

.stat-pill[data-tone="success"] {
  border-color: color-mix(in srgb, var(--success) 40%, var(--border-color));
}

.stat-pill[data-tone="warning"] {
  border-color: color-mix(in srgb, var(--warning) 45%, var(--border-color));
}

.stat-pill[data-tone="danger"] {
  border-color: color-mix(in srgb, var(--danger) 45%, var(--border-color));
}

.stat-pill__label {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-dim);
}

.stat-pill__value {
  font-size: 1.25rem;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  line-height: 1.15;
  color: var(--text-main);
}

.stat-pill[data-tone="success"] .stat-pill__value {
  color: var(--success);
}

.stat-pill[data-tone="warning"] .stat-pill__value {
  color: var(--warning);
}

.stat-pill[data-tone="danger"] .stat-pill__value {
  color: var(--danger);
}

.stat-pill__detail {
  font-size: 0.7rem;
  font-weight: 500;
  color: var(--text-muted);
  white-space: nowrap;
}

.kbd-shortcut {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.15rem 0.45rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-color);
  background: var(--bg-subtle);
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  line-height: 1.2;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  transition: border-color 0.15s ease, color 0.15s ease, background-color 0.15s ease;
}

.kbd-shortcut:hover {
  border-color: var(--border-highlight);
  color: var(--text-main);
  background: var(--bg-elevated);
}

/* --- Barre d'état compacte : Vue d'ensemble --- */
.overview-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}

.overview-toolbar__actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-left: auto;
}

.hero-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  width: fit-content;
  padding: 0.25rem 0.75rem;
  border-radius: var(--radius-pill);
  background-color: var(--bg-subtle);
  color: var(--text-dim);
  font-size: 0.75rem;
  font-weight: 600;
  border: 1px solid var(--border-color);
}

.hero-badge[data-tone="success"] {
  background-color: var(--success-bg);
  color: var(--success);
  border-color: color-mix(in srgb, var(--success) 35%, transparent);
}

.hero-badge[data-tone="warning"] {
  background-color: var(--warning-bg);
  color: var(--warning);
  border-color: color-mix(in srgb, var(--warning) 35%, transparent);
}

.hero-badge[data-tone="danger"] {
  background-color: var(--danger-bg);
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 35%, transparent);
}

/* Panneau repliable de la barre d'actions (widgets, filtres) */
.popover {
  position: relative;
}

.popover > summary {
  list-style: none;
  cursor: pointer;
}

.popover__panel {
  position: absolute;
  right: 0;
  z-index: 40;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-top: var(--space-1);
  padding: var(--space-3);
  min-width: 200px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
}

/* Grille de métriques et KPIs statistiques */
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: var(--space-3);
  margin-bottom: var(--space-4);
}

/* Grille bi-colonne pour la vue d'ensemble */
.overview-grid-2col {
  display: grid;
  grid-template-columns: 1.25fr 1fr;
  gap: var(--space-4);
  margin-bottom: var(--space-4);
}

.overview-grid-2col > .section-panel {
  margin-bottom: 0;
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

.insights-total {
  font-weight: 600;
  color: var(--text-main);
}

.segmented-distribution-bar {
  height: 12px;
  border-radius: 6px;
  background-color: var(--bg-subtle);
  display: flex;
  overflow: hidden;
  margin: var(--space-1) 0 var(--space-2) 0;
  border: 1px solid var(--border-color);
}

.segment-fill {
  height: 100%;
  transition: width 0.3s ease;
}

.segment-fill.mkv { background-color: var(--primary); }
.segment-fill.mp4 { background-color: var(--accent); }
.segment-fill.other { background-color: var(--warning); }
.segment-fill.default-segment { background-color: var(--border-color); width: 100%; }

.distribution-legend {
  display: flex;
  gap: 1.25rem;
  flex-wrap: wrap;
  font-size: 0.8125rem;
  color: var(--text-dim);
  margin-bottom: var(--space-3);
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
.legend-dot.mp4 { background-color: var(--accent); }
.legend-dot.other { background-color: var(--warning); }

.storage-breakdown {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-top: var(--space-2);
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

/* L'attribut hidden doit masquer l'élément même si un composant fixe display. */
[hidden] {
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

/* --- Adaptations Responsives Mobile (< 768px) --- */
@media (max-width: 768px) {
  html, body {
    overflow-x: hidden;
    max-width: 100vw;
  }

  .application-shell {
    overflow-x: hidden;
    max-width: 100vw;
  }

  .application-navigation-layout {
    overflow-x: hidden;
    max-width: 100vw;
    min-width: 0;
    padding: 0;
    gap: 0;
  }

  /* Étroit : la barre latérale devient un tiroir, ouvert par le bouton de la barre du haut. */
  .application-sidebar,
  .application-sidebar.is-collapsed {
    position: fixed;
    top: 0;
    bottom: 0;
    left: 0;
    height: 100dvh;
    width: min(18rem, 85vw);
    max-width: 85vw;
    border-radius: 0;
    border: 0;
    border-right: 1px solid var(--border-color);
    transform: translateX(-100%);
    visibility: hidden;
    transition: transform 0.2s ease-in-out, visibility 0.2s;
    z-index: 80;
    box-shadow: var(--shadow-md);
  }

  .application-sidebar.is-open {
    transform: translateX(0);
    visibility: visible;
  }

  .application-sidebar.is-collapsed .brand-info,
  .application-sidebar.is-collapsed .tab-btn-label,
  .application-sidebar.is-collapsed .tab-btn-badge,
  .application-sidebar.is-collapsed .nav-group-label,
  .application-sidebar.is-collapsed .sidebar-cmd-btn__label,
  .application-sidebar.is-collapsed .sidebar-cmd-btn__kbd,
  .application-sidebar.is-collapsed .sidebar-user-meta {
    display: revert;
  }

  .application-sidebar.is-collapsed .sidebar-user-card {
    flex-direction: row;
    padding: 0.45rem 0.55rem;
  }

  .application-sidebar.is-collapsed .sidebar-cmd-btn {
    justify-content: flex-start;
    padding: 0.45rem 0.6rem;
  }

  .sidebar-scrim {
    display: block;
    position: fixed;
    inset: 0;
    z-index: 70;
    border: 0;
    padding: 0;
    background: rgba(0, 0, 0, 0.45);
  }

  .sidebar-scrim[hidden] {
    display: none;
  }

  .topbar-menu-btn {
    display: inline-flex;
  }

  .sidebar-header {
    padding: 0.75rem 1rem;
    box-sizing: border-box;
  }

  .tab-navigation {
    padding: 0.5rem;
    min-width: 0;
    overflow-y: auto;
    box-sizing: border-box;
  }

  .tab-btn {
    width: 100%;
    min-height: 44px;
    white-space: nowrap;
  }

  .sidebar-collapse-btn {
    display: none;
  }

  .page-transition-region {
    max-width: 100vw;
    min-width: 0;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }

  .app-topbar {
    margin: 0;
  }

  .topbar-breadcrumb {
    display: none;
  }

  .topbar-container {
    padding: 0.35rem 0.5rem;
  }

  .topbar-search-anchor {
    max-width: none;
  }

  .main-content {
    padding: 1rem;
    max-width: 100vw;
    box-sizing: border-box;
  }
}
"#;
