//! Vue d'ensemble d'un worker : tuiles alimentées par `/api/v1/worker/overview` (ADR 0011).

pub const SCRIPT: &str = r###"
// --- Vue d'ensemble d'un worker : tâches du worker, relayées par le central ---
  const NODE_OVERVIEW_TILES = [
    { id: 'metric-files-count', field: 'queued', label: 'En attente', subtitle: 'Tâches en file pour ce worker' },
    { id: 'metric-storage-volume', field: 'running', label: 'En cours', subtitle: 'Traitements actifs sur ce worker' },
    { id: 'metric-success-rate', field: 'succeeded', label: 'Réussies', subtitle: 'Tâches menées à terme par ce worker' },
    { id: 'metric-workers-count', field: 'failed', label: 'En échec', subtitle: 'Tâches échouées sur ce worker' }
  ];

  function isWorkerInstance() {
    return Boolean(currentUser) && currentUser.instanceKind === 'worker';
  }

  function setElementText(root, selector, text) {
    const node = root ? root.querySelector(selector) : null;
    if (node) node.textContent = text;
  }

  // Adapte la vue d'ensemble : rien de ce qui n'existe que sur le central n'est affiché.
  function prepareWorkerDashboard(me) {
    const panel = el('panel-dashboard');
    if (!panel) return;
    const heroActions = panel.querySelector('.hero-actions');
    if (heroActions) heroActions.hidden = true;
    const insights = el('media-insights-title');
    const insightsCard = insights ? insights.closest('section') : null;
    if (insightsCard) insightsCard.hidden = true;
    setElementText(panel, '.hero-title', `Worker « ${me.instanceName || ''} »`);
    setElementText(panel, '.hero-subtitle', 'Tâches traitées par ce worker, relayées par le central.');
    NODE_OVERVIEW_TILES.forEach(tile => {
      const value = el(tile.id);
      const card = value ? value.closest('.metric-card') : null;
      setElementText(card, '.metric-label', tile.label);
      setElementText(card, '.metric-subtitle', tile.subtitle);
    });
    // Ni workers ni bibliothèques sur un worker : ces sources sont vides, pas en erreur.
    widgetSources.workers = 'ready';
    widgetSources.libraries = 'ready';
  }

  function setWorkerStatus(tone, text) {
    const badge = el('hero-status-badge');
    const label = el('hero-status-text');
    if (badge) badge.dataset.tone = tone;
    if (label) label.textContent = text;
  }

  async function loadWorkerOverview() {
    try {
      const res = await apiFetch('/api/v1/worker/overview');
      const overview = await res.json();
      NODE_OVERVIEW_TILES.forEach(tile => {
        const value = el(tile.id);
        if (value) value.textContent = String(overview.jobs[tile.field] ?? 0);
      });
      setWorkerStatus('success', overview.lastSeen
        ? `Central joignable • dernière activité ${formatDate(overview.lastSeen)}`
        : 'Central joignable • aucune activité enregistrée');
    } catch (e) {
      setWorkerStatus('danger', 'Central injoignable');
    }
  }
"###;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_targets_the_dashboard_tiles_and_the_overview_route() {
        for id in [
            "metric-files-count",
            "metric-storage-volume",
            "metric-success-rate",
            "metric-workers-count",
        ] {
            assert!(SCRIPT.contains(id), "{id}");
        }
        assert!(SCRIPT.contains("/api/v1/worker/overview"));
        assert!(SCRIPT.contains("function loadWorkerOverview"));
        assert!(SCRIPT.contains("function prepareWorkerDashboard"));
    }

    #[test]
    fn test_script_names_every_job_status_of_the_overview() {
        for field in ["queued", "running", "succeeded", "failed"] {
            assert!(SCRIPT.contains(&format!("field: '{field}'")), "{field}");
        }
    }
}
