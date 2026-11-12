//! Fonctions pures du poste de pilotage, testées en Rust puis émises dans le script.

pub const BYTE_UNITS: [&str; 5] = ["o", "Kio", "Mio", "Gio", "Tio"];
pub const VIDEO_FLOW_KEYWORDS: [&str; 4] = ["mkv", "vidéo", "mp4", "hevc"];

const HTML_ESCAPES: [(&str, &str); 5] = [
    ("&", "&amp;"),
    ("<", "&lt;"),
    (">", "&gt;"),
    ("\"", "&quot;"),
    ("'", "&#39;"),
];

const JOB_STATUS_CLASSES: [(&str, &str); 3] = [
    ("succeeded", "succeeded"),
    ("failed", "failed"),
    ("running", "running"),
];

pub fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        let fragment = ch.to_string();
        let replacement = HTML_ESCAPES
            .iter()
            .find(|(raw, _)| *raw == fragment)
            .map(|(_, entity)| *entity);
        if let Some(entity) = replacement {
            escaped.push_str(entity);
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return format!("0 {}", BYTE_UNITS[0]);
    }
    let mut index = 0;
    let mut scaled = bytes;
    while scaled >= 1024 && index < BYTE_UNITS.len() - 1 {
        scaled /= 1024;
        index += 1;
    }
    let divisor = 1024_u64.pow(index as u32) as f64;
    let rounded = ((bytes as f64 / divisor) * 100.0).round() / 100.0;
    format!("{} {}", trim_fixed(rounded), BYTE_UNITS[index])
}

fn trim_fixed(value: f64) -> String {
    let fixed = format!("{value:.2}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub fn job_status_class(status: &str) -> &'static str {
    JOB_STATUS_CLASSES
        .iter()
        .find(|(name, _)| *name == status)
        .map(|(_, class)| *class)
        .unwrap_or("queued")
}

pub fn matches_query(values: &[&str], query: &str) -> bool {
    let term = query.trim().to_lowercase();
    if term.is_empty() {
        return true;
    }
    values
        .iter()
        .any(|value| value.to_lowercase().contains(&term))
}

pub fn is_video_flow_name(name: &str) -> bool {
    let value = name.to_lowercase();
    VIDEO_FLOW_KEYWORDS
        .iter()
        .any(|keyword| value.contains(keyword))
}

pub fn count_video_flows<'a>(names: impl IntoIterator<Item = &'a str>) -> usize {
    names
        .into_iter()
        .filter(|name| is_video_flow_name(name))
        .count()
}

/// Script des fonctions pures et des aides de rendu partagées.
pub fn script() -> String {
    let escapes = HTML_ESCAPES
        .iter()
        .map(|(raw, entity)| {
            let pattern = match *raw {
                "&" => "/&/g",
                "<" => "/</g",
                ">" => "/>/g",
                "\"" => "/\"/g",
                "'" => "/'/g",
                _ => "/$/g",
            };
            let entity = serde_json::to_string(entity).unwrap_or_else(|_| "\"\"".to_owned());
            format!(".replace({pattern}, {entity})")
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    let units = serde_json::to_string(&BYTE_UNITS).unwrap_or_else(|_| "[]".to_owned());
    let statuses = JOB_STATUS_CLASSES
        .iter()
        .map(|(status, class)| format!("    case {status:?}: return {class:?};"))
        .collect::<Vec<_>>()
        .join("\n");
    let keywords = serde_json::to_string(&VIDEO_FLOW_KEYWORDS).unwrap_or_else(|_| "[]".to_owned());
    format!(
        r#"  function escapeHtml(str) {{
    if (!str) return '';
    return String(str)
    {escapes};
  }}

  function formatBytes(bytes) {{
    if (bytes === 0 || !bytes) return '0 o';
    const sizes = {units};
    let index = 0;
    let scaled = bytes;
    while (scaled >= 1024 && index < sizes.length - 1) {{
      scaled = Math.floor(scaled / 1024);
      index += 1;
    }}
    const divisor = Math.pow(1024, index);
    const rounded = Math.round((bytes / divisor) * 100) / 100;
    return String(rounded) + ' ' + sizes[index];
  }}

  function jobStatusClass(status) {{
    switch (status) {{
{statuses}
      default: return 'queued';
    }}
  }}

  function matchesQuery(values, query) {{
    const term = String(query || '').trim().toLowerCase();
    if (!term) return true;
    return (values || []).some(value => String(value || '').toLowerCase().includes(term));
  }}

  function isVideoFlowName(name) {{
    const value = String(name || '').toLowerCase();
    return {keywords}.some(keyword => value.includes(keyword));
  }}

  function textMatches(node, query) {{
    return matchesQuery([node && node.textContent], query);
  }}

  function setFiltered(node, visible) {{
    if (node) node.classList.toggle('is-filtered-out', !visible);
  }}

  function emptyStateRow(colspan, html, danger) {{
    const tone = danger ? ' text-danger' : '';
    return `<tr><td colspan="${{colspan}}" class="empty-state-text${{tone}}">${{html}}</td></tr>`;
  }}

  function emptyStateBlock(html, danger) {{
    const tone = danger ? ' text-danger' : '';
    return `<div class="empty-state-panel${{tone}}">${{html}}</div>`;
  }}

  function tableRow(cells) {{
    return `<tr>${{cells}}</tr>`;
  }}

  function inspectionEyebrow(factId) {{
    if (factId === 'files-fact') return 'Fichier';
    if (factId === 'flows-fact') return 'Flux';
    if (factId === 'repos-fact') return 'Dépôt';
    const catalog = {{
      app: 'Application',
      plugin: 'Plugin',
      flow: 'Flux publié',
      block: 'Bloc',
      pack: 'Pack',
      preset: 'Préréglage',
      resource: 'Ressource'
    }};
    const view = String(factId || '').replace('catalog-fact-', '');
    return catalog[view] || 'Dossier';
  }}

  function selectInspectionTab(drawer, tab) {{
    if (!drawer || !tab) return;
    drawer.querySelectorAll('.inspection-drawer__tab').forEach(item => {{
      const selected = item === tab;
      item.classList.toggle('is-active', selected);
      item.setAttribute('aria-selected', selected ? 'true' : 'false');
      item.tabIndex = selected ? 0 : -1;
      const pane = document.getElementById(item.getAttribute('aria-controls'));
      if (pane) pane.hidden = !selected;
    }});
  }}

  function closeInspectionDrawer(drawer) {{
    if (!drawer) return;
    drawer.classList.remove('is-open');
    drawer.setAttribute('aria-hidden', 'true');
    const action = drawer.querySelector('.inspection-panel__action');
    if (action) action.disabled = true;
    const first = drawer.querySelector('.inspection-drawer__tab');
    if (first) selectInspectionTab(drawer, first);
    const registre = drawer.closest('.registre');
    if (registre) {{
      registre.querySelectorAll('tr.is-open').forEach(row => row.classList.remove('is-open'));
    }}
  }}

  function showRegistreFact(factId, title, facts) {{
    const fact = document.getElementById(factId);
    const titleEl = document.getElementById(factId + '-title');
    const factsEl = document.getElementById(factId + '-facts');
    const eyebrowEl = document.getElementById(factId + '-eyebrow');
    const text = (facts || []).filter(Boolean).join(' · ');
    if (titleEl) titleEl.textContent = title || 'Sélectionnez une ligne';
    if (factsEl) factsEl.textContent = text || 'Le détail de la ligne apparaît ici.';
    if (eyebrowEl) eyebrowEl.textContent = inspectionEyebrow(factId);
    if (!fact) return;
    if (fact.classList.contains('inspection-drawer')) {{
      if (!title) {{
        closeInspectionDrawer(fact);
        return;
      }}
      fact.classList.add('is-open');
      fact.setAttribute('aria-hidden', 'false');
      const action = fact.querySelector('.inspection-panel__action');
      if (action) action.disabled = false;
      const first = fact.querySelector('.inspection-drawer__tab');
      if (first) selectInspectionTab(fact, first);
      const panel = fact.querySelector('.inspection-drawer__panel');
      if (panel) panel.focus();
      return;
    }}
    fact.hidden = false;
    fact.classList.toggle('is-filled', Boolean(title));
  }}

  function markRegistreRow(tbody, row) {{
    if (!tbody) return;
    tbody.querySelectorAll('tr.is-open').forEach(open => open.classList.remove('is-open'));
    if (row) row.classList.add('is-open');
  }}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_encodes_markup() {
        assert_eq!(escape_html(""), "");
        assert_eq!(escape_html("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&#39;");
        assert_eq!(escape_html("film.mkv"), "film.mkv");
    }

    #[test]
    fn format_bytes_uses_the_binary_units() {
        assert_eq!(format_bytes(0), "0 o");
        assert_eq!(format_bytes(500), "500 o");
        assert_eq!(format_bytes(1024), "1 Kio");
        assert_eq!(format_bytes(1536), "1.5 Kio");
        assert_eq!(format_bytes(1_048_576), "1 Mio");
        assert_eq!(format_bytes(1500), "1.46 Kio");
    }

    #[test]
    fn job_status_class_matches_the_known_states() {
        assert_eq!(job_status_class("succeeded"), "succeeded");
        assert_eq!(job_status_class("failed"), "failed");
        assert_eq!(job_status_class("running"), "running");
        assert_eq!(job_status_class("queued"), "queued");
        assert_eq!(job_status_class("pending"), "queued");
        assert_eq!(job_status_class(""), "queued");
    }

    #[test]
    fn matches_query_is_case_insensitive_and_empty_matches_all() {
        assert!(matches_query(&["film.MKV"], ""));
        assert!(matches_query(&["film.MKV"], "  mkv "));
        assert!(!matches_query(&["film.MKV"], "zzz"));
        assert!(matches_query(&["film.mkv", "abc"], "ABC"));
        assert!(matches_query(&[], ""));
        assert!(!matches_query(&[], "abc"));
    }

    #[test]
    fn video_flow_names_follow_the_keyword_list() {
        assert!(is_video_flow_name("Remux MKV"));
        assert!(is_video_flow_name("Optimisation Web MP4"));
        assert!(is_video_flow_name("Aiguillage HEVC"));
        assert!(is_video_flow_name("Piste vidéo"));
        assert!(!is_video_flow_name("Audio FLAC"));
        assert!(!is_video_flow_name("Remuxage Matroska propre"));
        assert_eq!(count_video_flows(["Remux MKV", "Audio FLAC", "HEVC"]), 2);
    }

    #[test]
    fn emitted_script_keeps_the_rust_tables() {
        let source = script();
        for unit in BYTE_UNITS {
            assert!(source.contains(unit));
        }
        for keyword in VIDEO_FLOW_KEYWORDS {
            assert!(source.contains(keyword));
        }
        for (status, class) in JOB_STATUS_CLASSES {
            assert!(source.contains(status));
            assert!(source.contains(class));
        }
        assert!(source.contains("function matchesQuery"));
        assert!(source.contains("function emptyStateRow"));
        assert!(source.contains("function isVideoFlowName"));
        assert!(source.contains("function tableRow"));
        assert!(source.contains("function showRegistreFact"));
        assert!(source.contains("function closeInspectionDrawer"));
        assert!(source.contains("function selectInspectionTab"));
        assert!(source.contains("function markRegistreRow"));
    }
}
