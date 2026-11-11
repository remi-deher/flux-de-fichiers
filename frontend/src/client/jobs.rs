//! Supervision des tâches et flux SSE.

pub const SCRIPT: &str = r###"
// --- Tâches de remuxage & Télémétrie SSE ---
  let jobsEventSource = null;
  let sseReconnectTimer = null;

  function updateSseBadge(status, label) {
    const badge = el('sse-status-badge');
    const text = el('sse-status-text');
    if (!badge || !text) return;
    badge.className = `status-pill ${status}`;
    text.textContent = label;
  }

  function startJobsSse() {
    if (jobsEventSource) {
      jobsEventSource.close();
      jobsEventSource = null;
    }
    if (sseReconnectTimer) {
      clearTimeout(sseReconnectTimer);
      sseReconnectTimer = null;
    }

    if (sessionMode !== 'cookie_session') {
      updateSseBadge('info', 'Sondage périodique');
      return;
    }

    try {
      jobsEventSource = new EventSource('/api/v1/jobs/events', { withCredentials: true });

      jobsEventSource.onopen = () => {
        updateSseBadge('success', 'En direct (SSE)');
      };

      const handleJobEvent = (e) => {
        try {
          const payload = JSON.parse(e.data);
          loadJobs();

          // Si la modale de détail est ouverte sur cette tâche, la rafraîchir en direct
          const dialog = el('job-detail-dialog');
          const idEl = el('detail-job-id');
          if (dialog && dialog.open && idEl && idEl.textContent === payload.job_id) {
            apiFetch(`/api/v1/jobs/${encodeURIComponent(payload.job_id)}`)
              .then(res => res.json())
              .then(job => {
                if (dialog.open) openJobDetailDialog(job.id);
              })
              .catch(() => {});
          }

          if (payload.event_type === 'job_succeeded') {
            showNotice(`Tâche terminée avec succès (${payload.job_id.slice(0, 8)}…)`, 'success');
          } else if (payload.event_type === 'job_failed') {
            showNotice(`Échec sur la tâche ${payload.job_id.slice(0, 8)}… (${payload.error_code || 'erreur'})`, 'warning');
          }
        } catch (_) {
          loadJobs();
        }
      };

      jobsEventSource.addEventListener('job_created', handleJobEvent);
      jobsEventSource.addEventListener('job_claimed', handleJobEvent);
      jobsEventSource.addEventListener('job_heartbeat', handleJobEvent);
      jobsEventSource.addEventListener('job_succeeded', handleJobEvent);
      jobsEventSource.addEventListener('job_failed', handleJobEvent);

      jobsEventSource.addEventListener('job_progress', (e) => {
        try {
          const payload = JSON.parse(e.data);
          const job = cachedJobs.find((j) => j.id === payload.job_id);
          if (job) {
            job.status = 'running';
            job.progress = payload.progress;
            renderJobsTable();
          }

          const dialog = el('job-detail-dialog');
          const idEl = el('detail-job-id');
          if (dialog && dialog.open && idEl && idEl.textContent === payload.job_id) {
            const statusEl = el('detail-job-status');
            if (statusEl && payload.progress !== undefined) {
              statusEl.innerHTML = `<span class="status-pill running"><span class="status-dot"></span>En cours (${payload.progress}%)</span>`;
            }
          }
        } catch (_) {}
      });

      jobsEventSource.onerror = () => {
        updateSseBadge('info', 'Reconnexion SSE…');
        if (jobsEventSource) {
          jobsEventSource.close();
          jobsEventSource = null;
        }
        sseReconnectTimer = setTimeout(() => {
          if (sessionMode === 'cookie_session') {
            startJobsSse();
          } else {
            updateSseBadge('info', 'Sondage périodique');
          }
        }, 5000);
      };
    } catch (_) {
      updateSseBadge('info', 'Sondage périodique');
    }
  }

  function stopJobsSse() {
    if (jobsEventSource) {
      jobsEventSource.close();
      jobsEventSource = null;
    }
    if (sseReconnectTimer) {
      clearTimeout(sseReconnectTimer);
      sseReconnectTimer = null;
    }
    updateSseBadge('info', 'Sondage périodique');
  }

  async function loadJobs() {
    const tbody = el('jobs-table-body');
    const metricJobs = el('metric-jobs-count');

    try {
      const res = await apiFetch('/api/v1/jobs');
      const data = await res.json();
      cachedJobs = Array.isArray(data) ? data : (data.jobs || []);

      if (metricJobs) metricJobs.textContent = String(cachedJobs.length);
      renderRecentJobs();
      renderJobsTable();
    } catch (e) {
      if (tbody) {
        tbody.innerHTML = `<tr><td colspan="5" class="empty-state-text" style="color: var(--danger-text);">
          ${escapeHtml(e.message === 'Non autorisé' ? 'Veuillez ouvrir une session pour afficher la file des tâches.' : 'Impossible de récupérer les tâches.')}
        </td></tr>`;
      }
    }
  }

  function renderRecentJobs() {
    const tbody = el('recent-jobs-table-body');
    if (!tbody) return;

    const recent = cachedJobs.slice(0, 3);
    if (recent.length === 0) {
      tbody.innerHTML = `<tr><td colspan="4" class="empty-state-text">Aucune tâche enregistrée pour le moment.</td></tr>`;
      return;
    }

    tbody.innerHTML = recent.map(j => {
      const statusClass = j.status === 'succeeded' ? 'succeeded' : (j.status === 'failed' ? 'failed' : (j.status === 'running' ? 'running' : 'queued'));
      return `
        <tr>
          <td style="font-family: var(--font-mono); font-weight: 600;">${escapeHtml(j.id.slice(0, 8))}…</td>
          <td>${escapeHtml(j.source_name)}</td>
          <td><span class="status-pill ${statusClass}"><span class="status-dot"></span>${escapeHtml(j.status)}</span></td>
          <td>${formatDate(j.created_at)}</td>
        </tr>
      `;
    }).join('');
  }

  function updateJobFiltersCount() {
    const counts = { all: cachedJobs.length, queued: 0, running: 0, succeeded: 0, failed: 0 };
    cachedJobs.forEach(j => {
      if (counts[j.status] !== undefined) counts[j.status]++;
    });

    Object.keys(counts).forEach(k => {
      const counterEl = el(`count-filter-${k}`);
      if (counterEl) counterEl.textContent = `(${counts[k]})`;
    });
  }

  function renderJobsTable() {
    const tbody = el('jobs-table-body');
    const searchInput = el('jobs-search-input');
    if (!tbody) return;

    updateJobFiltersCount();

    const query = searchInput ? searchInput.value.trim().toLowerCase() : '';
    let filtered = cachedJobs;

    if (activeJobFilter !== 'all') {
      filtered = filtered.filter(j => j.status === activeJobFilter);
    }

    if (query) {
      filtered = filtered.filter(j =>
        j.id.toLowerCase().includes(query) ||
        j.source_name.toLowerCase().includes(query)
      );
    }

    if (filtered.length === 0) {
      tbody.innerHTML = `<tr><td colspan="5" class="empty-state-text">Aucune tâche ne correspond à ces critères.</td></tr>`;
      return;
    }

    tbody.innerHTML = filtered.map(j => {
      const statusClass = j.status === 'succeeded' ? 'succeeded' : (j.status === 'failed' ? 'failed' : (j.status === 'running' ? 'running' : 'queued'));
      const statusLabel = j.status === 'running' && j.progress !== undefined ? `En cours (${j.progress}%)` : escapeHtml(j.status);
      const canDownload = j.status === 'succeeded' && sessionMode === 'cookie_session';
      const dlBtnHtml = canDownload
        ? `<button type="button" class="btn btn-secondary btn-sm job-download-btn" data-job-id="${escapeHtml(j.id)}" data-output="${escapeHtml(j.output_name || 'resultat.mkv')}" title="Télécharger le flux MKV directement sans charger la RAM">Télécharger</button>`
        : '';

      return `
        <tr>
          <td style="font-family: var(--font-mono); font-weight: 600;">${escapeHtml(j.id.slice(0, 8))}…</td>
          <td>${escapeHtml(j.source_name)}</td>
          <td><span class="status-pill ${statusClass}"><span class="status-dot"></span>${statusLabel}</span></td>
          <td>${formatDate(j.created_at)}</td>
          <td>
            <div style="display: flex; gap: 0.5rem; align-items: center;">
              <button type="button" class="btn btn-secondary btn-sm job-detail-btn" data-job-id="${escapeHtml(j.id)}">Détails</button>
              ${dlBtnHtml}
            </div>
          </td>
        </tr>
      `;
    }).join('');

    tbody.querySelectorAll('.job-detail-btn').forEach(b => {
      b.onclick = () => openJobDetailDialog(b.getAttribute('data-job-id'));
    });

    tbody.querySelectorAll('.job-download-btn').forEach(b => {
      b.onclick = () => downloadJobStream(b.getAttribute('data-job-id'), b.getAttribute('data-output'));
    });
  }

  // --- Téléchargement en flux direct (sans Blob en mémoire RAM) ---
  function downloadJobStream(jobId, filename) {
    if (!jobId) return;

    if (sessionMode !== 'cookie_session') {
      showNotice('Le téléchargement en flux direct nécessite l\'ouverture d\'une session web par cookie HttpOnly (#29). Ouvrez une session dans l\'onglet Paramètres ou utilisez la commande curl.', 'warning');
      return;
    }

    const downloadUrl = `/api/v1/jobs/${encodeURIComponent(jobId)}/output`;
    showNotice(`Démarrage du téléchargement direct : ${filename}…`, 'info');

    const link = document.createElement('a');
    link.href = downloadUrl;
    link.setAttribute('download', filename || `job-${jobId}.mkv`);
    link.style.display = 'none';
    document.body.appendChild(link);
    link.click();

    setTimeout(() => {
      document.body.removeChild(link);
    }, 1500);
  }

  // --- Modale de détails d'une tâche ---
  function openJobDetailDialog(jobId) {
    const dialog = el('job-detail-dialog');
    if (!dialog) return;

    const job = cachedJobs.find(j => j.id === jobId);
    if (!job) return;

    const idEl = el('detail-job-id');
    const sourceEl = el('detail-job-source');
    const statusEl = el('detail-job-status');
    const workerEl = el('detail-job-worker');
    const attemptsEl = el('detail-job-attempts');
    const createdEl = el('detail-job-created');
    const outputEl = el('detail-job-output');
    const errorBox = el('detail-error-box');
    const errorContent = el('detail-error-content');
    const curlSec = el('detail-curl-section');
    const curlCmd = el('detail-curl-command');
    const dlBtn = el('job-dialog-download-btn');

    if (idEl) idEl.textContent = job.id;
    if (sourceEl) sourceEl.textContent = job.source_name;
    if (statusEl) {
      const statusClass = job.status === 'succeeded' ? 'succeeded' : (job.status === 'failed' ? 'failed' : (job.status === 'running' ? 'running' : 'queued'));
      statusEl.innerHTML = `<span class="status-pill ${statusClass}"><span class="status-dot"></span>${escapeHtml(job.status)}</span>`;
    }
    if (workerEl) workerEl.textContent = job.worker_id || job.target_worker_id || 'N\'importe quel agent (ou central autonome)';
    if (attemptsEl) attemptsEl.textContent = `${job.attempts || 1} tentative(s)`;
    if (createdEl) createdEl.textContent = formatDate(job.created_at);
    if (outputEl) outputEl.textContent = job.output_name || 'Non généré';

    if (errorBox && errorContent) {
      if (job.status === 'failed' || job.error_code) {
        errorBox.style.display = 'block';
        errorContent.textContent = job.error_code
          ? `Code d'erreur: ${job.error_code}`
          : 'Traitement en échec sans code spécifique.';
      } else {
        errorBox.style.display = 'none';
      }
    }

    if (curlSec && curlCmd) {
      const origin = window.location.origin || 'http://localhost:3000';
      curlCmd.textContent = `curl -H "Authorization: Bearer <ADMIN_KEY>" "${origin}/api/v1/jobs/${encodeURIComponent(job.id)}/output" -o "${job.output_name || 'resultat.mkv'}"`;
      curlSec.style.display = 'block';
    }

    if (dlBtn) {
      if (job.status === 'succeeded' && sessionMode === 'cookie_session') {
        dlBtn.style.display = 'inline-flex';
        dlBtn.textContent = 'Télécharger le résultat (flux direct)';
        dlBtn.onclick = () => {
          downloadJobStream(job.id, job.output_name || 'resultat.mkv');
        };
      } else {
        dlBtn.style.display = 'none';
      }
    }

    dialog.showModal();
    const closeBtn = el('job-dialog-close-btn');
    if (closeBtn) closeBtn.focus();
  }

  function closeJobDetailDialog() {
    const dialog = el('job-detail-dialog');
    if (dialog && dialog.open) {
      dialog.close();
    }
  }

  // --- Gestion des sous-vues File active, Historique et Diagnostic ---
  function setupActivityRoleTabs() {
    const viewOps = el('activity-view-operations');
    const viewDiag = el('activity-view-diagnostic');
    const subtabActive = el('subtab-jobs-active');
    const subtabHistory = el('subtab-jobs-history');
    const subtabDiag = el('subtab-jobs-diagnostic');

    document.addEventListener('flux:subnav-change', (e) => {
      const targetId = e.detail && e.detail.targetId;
      if (!targetId || !targetId.startsWith('subtab-jobs-')) return;

      if (targetId === 'subtab-jobs-active') {
        if (viewOps && subtabActive) subtabActive.appendChild(viewOps);
        if (viewOps) viewOps.style.display = 'block';
        if (subtabDiag) subtabDiag.setAttribute('hidden', '');
        if (subtabHistory) subtabHistory.setAttribute('hidden', '');
        if (subtabActive) subtabActive.removeAttribute('hidden');
        jobsFilter = 'running';
        updateFilterChips();
        renderJobsTable();
      } else if (targetId === 'subtab-jobs-history') {
        if (viewOps && subtabHistory) subtabHistory.appendChild(viewOps);
        if (viewOps) viewOps.style.display = 'block';
        if (subtabActive) subtabActive.setAttribute('hidden', '');
        if (subtabDiag) subtabDiag.setAttribute('hidden', '');
        if (subtabHistory) subtabHistory.removeAttribute('hidden');
        jobsFilter = 'succeeded';
        updateFilterChips();
        renderJobsTable();
      } else if (targetId === 'subtab-jobs-diagnostic') {
        if (viewOps && subtabDiag) subtabDiag.appendChild(viewOps);
        if (viewOps) viewOps.style.display = 'block';
        if (subtabActive) subtabActive.setAttribute('hidden', '');
        if (subtabHistory) subtabHistory.setAttribute('hidden', '');
        if (subtabDiag) subtabDiag.removeAttribute('hidden');
        jobsFilter = 'failed';
        updateFilterChips();
        renderJobsTable();
      }
    });

    const refreshBtn = el('jobs-refresh-btn');
    if (refreshBtn) {
      refreshBtn.addEventListener('click', loadJobs);
    }
  }

  document.addEventListener('DOMContentLoaded', setupActivityRoleTabs);

"###;
