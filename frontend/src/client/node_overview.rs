//! Vue d'ensemble d'un worker : tuiles alimentées par `/api/v1/node/overview` (ADR 0011).

pub const SCRIPT: &str = r###"
// --- Vue d'ensemble d'un worker : tâches du nœud, relayées par la centrale ---
  const NODE_OVERVIEW_TILES = [
    { id: 'metric-files-count', field: 'queued', label: 'En attente', subtitle: 'Tâches en file pour ce nœud' },
    { id: 'metric-storage-volume', field: 'running', label: 'En cours', subtitle: 'Traitements actifs sur ce nœud' },
    { id: 'metric-success-rate', field: 'succeeded', label: 'Réussies', subtitle: 'Tâches menées à terme par ce nœud' },
    { id: 'metric-workers-count', field: 'failed', label: 'En échec', subtitle: 'Tâches échouées sur ce nœud' }
  ];

  function isWorkerNode() {
    return Boolean(currentUser) && currentUser.nodeKind === 'worker';
  }

  function setNodeText(root, selector, text) {
    const node = root ? root.querySelector(selector) : null;
    if (node) node.textContent = text;
  }

  // Adapte la vue d'ensemble : rien de ce qui n'existe que sur la centrale n'est affiché.
  function prepareWorkerDashboard(me) {
    const panel = el('panel-dashboard');
    if (!panel) return;
    const heroActions = panel.querySelector('.hero-actions');
    if (heroActions) heroActions.hidden = true;
    const insights = el('media-insights-title');
    const insightsCard = insights ? insights.closest('section') : null;
    if (insightsCard) insightsCard.hidden = true;
    setNodeText(panel, '.hero-title', `Nœud « ${me.instanceName || ''} »`);
    setNodeText(panel, '.hero-subtitle', 'Tâches traitées par ce nœud, relayées par la centrale.');
    NODE_OVERVIEW_TILES.forEach(tile => {
      const value = el(tile.id);
      const card = value ? value.closest('.metric-card') : null;
      setNodeText(card, '.metric-label', tile.label);
      setNodeText(card, '.metric-subtitle', tile.subtitle);
    });
    // Ni nœuds ni bibliothèques sur un worker : ces sources sont vides, pas en erreur.
    widgetSources.workers = 'ready';
    widgetSources.libraries = 'ready';
  }

  function setNodeStatus(tone, text) {
    const badge = el('hero-status-badge');
    const label = el('hero-status-text');
    if (badge) badge.dataset.tone = tone;
    if (label) label.textContent = text;
  }

  async function loadNodeOverview() {
    try {
      const res = await apiFetch('/api/v1/node/overview');
      const overview = await res.json();
      NODE_OVERVIEW_TILES.forEach(tile => {
        const value = el(tile.id);
        if (value) value.textContent = String(overview.jobs[tile.field] ?? 0);
      });
      setNodeStatus('success', overview.lastSeen
        ? `Centrale joignable • dernière activité ${formatDate(overview.lastSeen)}`
        : 'Centrale joignable • aucune activité enregistrée');
    } catch (e) {
      setNodeStatus('danger', 'Centrale injoignable');
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
        assert!(SCRIPT.contains("/api/v1/node/overview"));
        assert!(SCRIPT.contains("function loadNodeOverview"));
        assert!(SCRIPT.contains("function prepareWorkerDashboard"));
    }

    #[test]
    fn test_script_names_every_job_status_of_the_overview() {
        for field in ["queued", "running", "succeeded", "failed"] {
            assert!(SCRIPT.contains(&format!("field: '{field}'")), "{field}");
        }
    }
}
