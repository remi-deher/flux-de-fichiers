//! Widgets : sources de données, états et rendu par type, pour toutes les pages qui en accueillent.

pub const SCRIPT: &str = r###"
// --- Widgets : état des sources, rendu par type (clés = WidgetKind) ---
  const WORKER_ONLINE_WINDOW_MS = 120000;
  const HOUR_MS = 3600000;
  const ACTIVITY_WINDOW_HOURS = 24;
  const ATTENTION_FAILED_LIMIT = 5;
  const RUNNING_LIMIT = 6;
  const PERCENT = 100;
  const WIDGET_ERROR_MESSAGE = 'Données indisponibles : ouvrez une session ou vérifiez le central.';

  // État de chaque source de données : 'pending' | 'ready' | 'error'.
  const widgetSources = { jobs: 'pending', workers: 'pending', libraries: 'pending', tools: 'pending' };

  // Sources dont dépend chaque type de widget.
  const WIDGET_SOURCES = {
    attention: ['jobs', 'workers', 'libraries', 'tools'],
    running: ['jobs'],
    activity: ['jobs'],
    fleet: ['workers'],
    libraries: ['libraries'],
    tools: ['tools']
  };

  const WIDGET_EMPTY_MESSAGES = {
    attention: 'Rien à signaler.',
    running: 'Aucun traitement en cours.',
    activity: 'Aucune tâche sur les 24 dernières heures.',
    libraries: 'Aucune bibliothèque déclarée.',
    tools: 'Aucun outil connu.'
  };

  function markWidgetSource(name, ok) {
    widgetSources[name] = ok ? 'ready' : 'error';
    renderWidgets();
  }

  function isWorkerOnline(worker) {
    return (Date.now() - new Date(worker.last_seen).getTime()) < WORKER_ONLINE_WINDOW_MS;
  }

  function isLocalCentral(worker) {
    return String(worker.name || '').startsWith('central');
  }

  // Une ligne de widget : pastille de ton, libellé, progression et métadonnée optionnelles.
  // `goto` est le slug d'une page ; la ligne devient alors un bouton de navigation.
  function widgetRow(item) {
    const tone = item.tone || 'neutral';
    const tag = item.goto ? 'button' : 'div';
    const attrs = item.goto ? ` type="button" data-goto="${escapeHtml(item.goto)}"` : '';
    const progress = item.progress !== undefined
      ? `<progress class="widget-progress" max="${PERCENT}" value="${item.progress}"></progress>`
      : '';
    return `<li><${tag} class="widget-row" data-tone="${tone}"${attrs}>
        <span class="widget-row__dot" aria-hidden="true"></span>
        <span class="widget-row__label">${escapeHtml(item.label)}</span>
        ${progress}
        <span class="widget-row__meta">${escapeHtml(item.meta || '')}</span>
      </${tag}></li>`;
  }

  function widgetList(items) {
    return `<ul class="widget-list">${items.map(widgetRow).join('')}</ul>`;
  }

  // Chaque rendu renvoie le HTML du corps, ou null quand il n'y a rien à montrer.
  function renderWidgetAttention() {
    const items = [];
    cachedJobs
      .filter(job => job.status === 'failed')
      .sort((a, b) => new Date(b.created_at) - new Date(a.created_at))
      .slice(0, ATTENTION_FAILED_LIMIT)
      .forEach(job => items.push({ tone: 'danger', label: `Échec : ${job.source_name}`, meta: job.error_code || 'erreur', goto: 'jobs' }));
    cachedWorkers
      .filter(worker => !isLocalCentral(worker) && !isWorkerOnline(worker))
      .forEach(worker => items.push({ tone: 'warning', label: `Worker hors ligne : ${worker.name || worker.id}`, meta: formatDate(worker.last_seen), goto: 'workers' }));
    cachedLibraries
      .filter(library => libraryReadiness(library) !== 'Prête')
      .forEach(library => items.push({ tone: 'warning', label: `À configurer : ${library.name}`, goto: 'files' }));
    cachedToolStatus
      .filter(tool => tool.status !== 'ready' && tool.status !== TOOL_STATUS_INSTALLING)
      .forEach(tool => items.push({ tone: 'warning', label: `Outil absent : ${tool.id}`, goto: 'plugins' }));
    return items.length ? widgetList(items) : null;
  }

  function renderWidgetRunning() {
    const running = cachedJobs.filter(job => job.status === 'running');
    const queued = cachedJobs.filter(job => job.status === 'queued').length;
    if (running.length === 0 && queued === 0) return null;
    const items = running.slice(0, RUNNING_LIMIT).map(job => {
      const percent = Math.round(job.progress || 0);
      return { tone: 'success', label: job.source_name, progress: percent, meta: `${percent}%`, goto: 'jobs' };
    });
    if (queued > 0) items.push({ label: `${queued} en attente`, goto: 'jobs' });
    return widgetList(items);
  }

  function renderWidgetActivity() {
    const now = Date.now();
    const buckets = Array.from({ length: ACTIVITY_WINDOW_HOURS }, () => ({ total: 0, failed: 0 }));
    cachedJobs.forEach(job => {
      const age = now - new Date(job.created_at).getTime();
      const index = ACTIVITY_WINDOW_HOURS - 1 - Math.floor(age / HOUR_MS);
      if (age < 0 || index < 0 || index >= ACTIVITY_WINDOW_HOURS) return;
      buckets[index].total += 1;
      if (job.status === 'failed') buckets[index].failed += 1;
    });
    const peak = Math.max(...buckets.map(bucket => bucket.total));
    if (peak === 0) return null;
    const bars = buckets.map((bucket, index) => {
      const hour = new Date(now - (ACTIVITY_WINDOW_HOURS - 1 - index) * HOUR_MS).getHours();
      const tone = bucket.failed > 0 ? 'danger' : (bucket.total > 0 ? 'success' : 'neutral');
      const height = Math.round((bucket.total / peak) * PERCENT);
      return `<span class="widget-bars__bar" data-tone="${tone}" data-height="${height}" title="${hour} h : ${bucket.total} tâche(s)"></span>`;
    }).join('');
    return `<div class="widget-bars" role="img" aria-label="Tâches créées par heure, sur les ${ACTIVITY_WINDOW_HOURS} dernières heures">${bars}</div>
      <div class="widget-bars__axis"><span>-${ACTIVITY_WINDOW_HOURS} h</span><span>maintenant</span></div>`;
  }

  function renderWidgetFleet() {
    if (cachedWorkers.length === 0) {
      return widgetList([{ tone: 'success', label: 'Central', meta: 'Intégré', goto: 'workers' }]);
    }
    return widgetList(cachedWorkers.map(worker => {
      const online = isWorkerOnline(worker);
      return { tone: online ? 'success' : 'danger', label: worker.name || worker.id, meta: online ? 'En ligne' : formatDate(worker.last_seen), goto: 'workers' };
    }));
  }

  function renderWidgetLibraries() {
    if (cachedLibraries.length === 0) return null;
    return widgetList(cachedLibraries.map(library => {
      const workers = Array.isArray(library.workers) ? library.workers.length : 0;
      const ready = libraryReadiness(library) === 'Prête';
      return { tone: ready ? 'success' : 'warning', label: library.name, meta: `${workers} worker(s)`, goto: 'files' };
    }));
  }

  function renderWidgetTools() {
    if (cachedToolStatus.length === 0) return null;
    return widgetList(cachedToolStatus.map(tool => {
      const tone = tool.status === 'ready' ? 'success' : (tool.status === TOOL_STATUS_INSTALLING ? 'warning' : 'danger');
      return { tone, label: tool.id, meta: toolStatusOf(tool.id), goto: 'plugins' };
    }));
  }

  const WIDGET_RENDERERS = {
    attention: renderWidgetAttention,
    running: renderWidgetRunning,
    activity: renderWidgetActivity,
    fleet: renderWidgetFleet,
    libraries: renderWidgetLibraries,
    tools: renderWidgetTools
  };

  function setWidgetState(node, state, text) {
    node.dataset.state = state;
    const message = node.querySelector('[data-widget-message]');
    if (message) message.textContent = text;
  }

  // Met à jour chaque widget de la page : chargement, erreur, vide ou prêt selon ses sources.
  function renderWidgets() {
    document.querySelectorAll('[data-widget]').forEach(node => {
      const kind = node.dataset.widget;
      const render = WIDGET_RENDERERS[kind];
      const sources = WIDGET_SOURCES[kind];
      const body = node.querySelector('[data-widget-body]');
      if (!render || !sources || !body) return;

      const states = sources.map(name => widgetSources[name]);
      if (states.includes('error')) return setWidgetState(node, 'error', WIDGET_ERROR_MESSAGE);
      if (states.includes('pending')) return setWidgetState(node, 'loading', '');

      const html = render();
      if (html === null) {
        body.innerHTML = '';
        return setWidgetState(node, 'empty', WIDGET_EMPTY_MESSAGES[kind] || '');
      }
      body.innerHTML = html;
      body.querySelectorAll('[data-height]').forEach(bar => {
        bar.style.setProperty('--bar-height', `${bar.dataset.height}%`);
      });
      setWidgetState(node, 'ready', '');
    });
  }

  // Tout élément porteur de data-goto ouvre la page indiquée (lignes de widget, liens d'en-tête).
  document.addEventListener('click', (event) => {
    const target = event.target.closest('[data-goto]');
    if (target) switchTab(target.dataset.goto);
  });
"###;
