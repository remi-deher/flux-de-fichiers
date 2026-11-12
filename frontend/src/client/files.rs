//! Comptage des médias sources pour la vue d'ensemble.

pub const SCRIPT: &str = r###"
  async function loadFiles() {
    const metricCount = el('metric-files-count');
    try {
      const res = await apiFetch('/api/v1/files');
      const data = await res.json();
      cachedFiles = Array.isArray(data) ? data : (data.files || []);
      if (metricCount) metricCount.textContent = String(cachedFiles.length);
      if (typeof updateOverviewStats === 'function') updateOverviewStats();
    } catch (e) {
      if (metricCount && e && e.message === 'Non autorisé') metricCount.textContent = '—';
    }
  }
"###;
