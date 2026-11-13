//! Supervision des tâches et flux SSE.

pub const SCRIPT: &str = r###"
// --- Tâches de remuxage & Télémétrie SSE ---
  let jobsEventSource = null;
  let sseReconnectTimer = null;

  function startJobsSse() {
    if (jobsEventSource) {
      jobsEventSource.close();
      jobsEventSource = null;
    }
    if (sseReconnectTimer) {
      clearTimeout(sseReconnectTimer);
      sseReconnectTimer = null;
    }

    if (sessionMode !== 'cookie_session') return;

    try {
      jobsEventSource = new EventSource('/api/v1/jobs/events', { withCredentials: true });

      const handleJobEvent = (e) => {
        try {
          const payload = JSON.parse(e.data);
          loadJobs();

          // Un worker a réclamé une tâche ou émis un heartbeat : actualiser les workers
          if (payload.event_type === 'job_claimed' || payload.event_type === 'job_heartbeat') {
            loadWorkers();
          }

          // Fin d'un traitement : actualiser immédiatement les fichiers pour afficher le résultat
          if (payload.event_type === 'job_succeeded') {
            loadFiles();
            showNotice(`Tâche terminée avec succès (${payload.job_id.slice(0, 8)}…)`, 'success');
          } else if (payload.event_type === 'job_failed') {
            showNotice(`Échec sur la tâche ${payload.job_id.slice(0, 8)}… (${payload.error_code || 'erreur'})`, 'warning');
          }

          // Si le tiroir est ouvert sur cette tâche, le rafraîchir en direct
          if (isJobDrawerOpenFor(payload.job_id)) {
            apiFetch(`/api/v1/jobs/${encodeURIComponent(payload.job_id)}`)
              .then(res => res.json())
              .then(job => {
                if (isJobDrawerOpenFor(job.id)) fillJobDrawer(job);
              })
              .catch(() => {});
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
      jobsEventSource.addEventListener('job_canceled', handleJobEvent);
      jobsEventSource.addEventListener('job_retried', handleJobEvent);

      jobsEventSource.addEventListener('job_progress', (e) => {
        try {
          const payload = JSON.parse(e.data);
          const job = cachedJobs.find((j) => j.id === payload.job_id);
          if (job) {
            job.status = 'running';
            job.progress = payload.progress;
            renderJobsTable();
            renderWidgets();
          }

          if (isJobDrawerOpenFor(payload.job_id)) {
            const statusEl = el('detail-job-status');
            if (statusEl && payload.progress !== undefined) {
              statusEl.innerHTML = renderStatusPill(`En cours (${payload.progress}%)`, 'running', { pulse: true });
            }
          }
        } catch (_) {}
      });

      jobsEventSource.onerror = () => {
        if (jobsEventSource) {
          jobsEventSource.close();
          jobsEventSource = null;
        }
        sseReconnectTimer = setTimeout(() => {
          if (sessionMode === 'cookie_session') startJobsSse();
        }, 5000);
      };
    } catch (_) {}
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
      if (typeof updateOverviewStats === 'function') updateOverviewStats();
      markWidgetSource('jobs', true);
    } catch (e) {
      markWidgetSource('jobs', false);
      if (tbody) {
        tbody.innerHTML = emptyStateRow(5, escapeHtml(e.message === 'Non autorisé' ? 'Veuillez ouvrir une session pour afficher la file des tâches.' : 'Impossible de récupérer les tâches.'), true);
      }
    }
  }

  function renderRecentJobs() {
    const tbody = el('recent-jobs-table-body');
    if (!tbody) return;

    const recent = cachedJobs.slice(0, 3);
    if (recent.length === 0) {
      tbody.innerHTML = emptyStateRow(4, 'Aucune tâche enregistrée pour le moment.', false);
      return;
    }

    tbody.innerHTML = recent.map(j => {
      const statusClass = jobStatusClass(j.status);
      const isRunning = j.status === 'running';
      return tableRow(`
          <td class="text-mono-strong">${escapeHtml(j.id.slice(0, 8))}…</td>
          <td>${escapeHtml(j.source_name)}</td>
          <td>${renderStatusPill(escapeHtml(j.status), statusClass, { pulse: isRunning })}</td>
          <td>${formatDate(j.created_at)}</td>
      `);
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
      filtered = filtered.filter(j => matchesQuery([j.id, j.source_name], query));
    }

    if (filtered.length === 0) {
      tbody.innerHTML = emptyStateRow(5, 'Aucune tâche ne correspond à ces critères.', false);
      return;
    }

    tbody.innerHTML = filtered.map(j => {
      const statusClass = jobStatusClass(j.status);
      const isRunning = j.status === 'running';
      const statusLabel = isRunning && j.progress !== undefined ? `En cours (${j.progress}%)` : escapeHtml(j.status);

      return `<tr data-job-id="${escapeHtml(j.id)}">
          <td class="text-mono-strong">${escapeHtml(j.id.slice(0, 8))}…</td>
          <td>${escapeHtml(j.source_name)}</td>
          <td>${renderStatusPill(statusLabel, statusClass, { pulse: isRunning })}</td>
          <td>${formatDate(j.created_at)}</td>
          <td class="cell-end">
            <button type="button" class="btn btn-secondary btn-sm registre-open-btn" data-job-id="${escapeHtml(j.id)}">Ouvrir</button>
          </td>
      </tr>`;
    }).join('');

    const openId = openJobDrawerId();
    const openRow = openId ? tbody.querySelector(`tr[data-job-id="${openId}"]`) : null;
    if (openRow) openRow.classList.add('is-open');

    tbody.querySelectorAll('.registre-open-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        const job = cachedJobs.find(j => j.id === btn.getAttribute('data-job-id'));
        if (!job) return;
        markRegistreRow(tbody, btn.closest('tr'));
        openJob(job);
      });
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

  // --- Tiroir de détails d'une tâche ---
  function openJobDrawerId() {
    const drawer = el('jobs-fact');
    const idEl = el('detail-job-id');
    return drawer && drawer.classList.contains('is-open') && idEl ? idEl.textContent : '';
  }

  function isJobDrawerOpenFor(jobId) {
    return Boolean(jobId) && openJobDrawerId() === jobId;
  }

  function openJob(job) {
    const status = job.status === 'running' && job.progress !== undefined
      ? `En cours (${job.progress}%)`
      : job.status;
    showRegistreFact('jobs-fact', job.source_name, [status, formatDate(job.created_at)]);
    fillJobDrawer(job);
  }

  function fillJobDrawer(job) {
    const setText = (id, text) => {
      const node = el(id);
      if (node) node.textContent = text;
    };
    setText('detail-job-id', job.id);
    setText('detail-job-source', job.source_name);
    setText('detail-job-worker', job.worker_id || job.target_worker_id || 'N\'importe quel worker (ou central autonome)');
    setText('detail-job-attempts', `${job.attempts || 1} tentative(s)`);
    setText('detail-job-created', formatDate(job.created_at));
    setText('detail-job-output', job.output_name || 'Non généré');

    const statusEl = el('detail-job-status');
    if (statusEl) {
      statusEl.innerHTML = renderStatusPill(escapeHtml(job.status), jobStatusClass(job.status), { pulse: job.status === 'running' });
    }

    const errorBox = el('detail-error-box');
    if (errorBox) {
      errorBox.hidden = !(job.status === 'failed' || job.error_code);
      setText('detail-error-content', job.error_code
        ? `Code d'erreur: ${job.error_code}`
        : 'Traitement en échec sans code spécifique.');
    }

    const origin = window.location.origin || 'http://localhost:3000';
    setText('detail-curl-command', `curl -H "Authorization: Bearer <ADMIN_KEY>" "${origin}/api/v1/jobs/${encodeURIComponent(job.id)}/output" -o "${job.output_name || 'resultat.mkv'}"`);

    const dlBtn = el('jobs-fact-download-btn');
    if (dlBtn) {
      const canDownload = job.status === 'succeeded' && sessionMode === 'cookie_session';
      dlBtn.hidden = !canDownload;
      dlBtn.onclick = canDownload
        ? () => downloadJobStream(job.id, job.output_name || 'resultat.mkv')
        : null;
    }
  }

  // --- Filtre de statut commun : onglets d'Activité et feuille de filtres ---
  function setJobFilter(filter) {
    activeJobFilter = filter;
    document.querySelectorAll('.filter-chip[data-filter]').forEach(chip => {
      chip.setAttribute('aria-pressed', chip.getAttribute('data-filter') === filter ? 'true' : 'false');
    });
    renderJobsTable();
    updateFilterCount();
  }

  // Chaque onglet d'Activité applique un filtre de statut au même registre.
  const JOBS_VIEW_FILTERS = {
    'btn-subtab-jobs-active': 'running',
    'btn-subtab-jobs-history': 'succeeded',
    'btn-subtab-jobs-diagnostic': 'failed'
  };

  function setupActivityRoleTabs() {
    document.addEventListener('flux:subnav-change', (e) => {
      const tabBtn = e.detail && e.detail.tabBtn;
      const filter = tabBtn ? JOBS_VIEW_FILTERS[tabBtn.id] : null;
      if (filter) setJobFilter(filter);
    });
  }

  document.addEventListener('DOMContentLoaded', setupActivityRoleTabs);

"###;
