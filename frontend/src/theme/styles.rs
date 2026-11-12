//! Générateur de styles globaux et design tokens CSS en Rust typé.

use super::tokens::{DARK_THEME, LIGHT_THEME};

/// Génère la feuille de style globale contenant les variables de design tokens et les resets racine.
pub fn generate_global_tokens_css() -> String {
    format!(
        r#"
:root {{
  /* --- Safe Area Insets (Apple HIG & WebKit) --- */
  --sat: env(safe-area-inset-top, 0px);
  --sar: env(safe-area-inset-right, 0px);
  --sab: env(safe-area-inset-bottom, 0px);
  --sal: env(safe-area-inset-left, 0px);

  /* --- Cibles tactiles minimales (Google M3 48px & Apple HIG 44px) --- */
  --touch-target-min: 48px;
  --touch-target-compact: 44px;

  /* --- Surfaces & Couleurs Thème Sombre --- */
  --bg-app: {dark_bg_app};
  --bg-card: {dark_bg_card};
  --bg-subtle: {dark_bg_subtle};
  --border-color: {dark_border};
  --text-main: {dark_text};
  --text-dim: {dark_text_dim};
  --text-muted: {dark_text_muted};
  --primary: {dark_primary};
  --primary-contrast: {dark_primary_contrast};
  --primary-bg: {dark_primary_bg};
  --accent: {dark_accent};
  --success: {dark_success};
  --success-bg: {dark_success_bg};
  --warning: {dark_warning};
  --warning-bg: {dark_warning_bg};
  --danger: {dark_danger};
  --danger-bg: {dark_danger_bg};
  --info: {dark_info};
  --info-bg: {dark_info_bg};
  --radius-sm: {radius_sm};
  --radius-md: {radius_md};
  --radius-lg: {radius_lg};
  --radius-pill: {radius_pill};
  --shadow-sm: {shadow_sm};
  --shadow-md: {shadow_md};
  --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji";
  --font-mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
}}

@media (prefers-color-scheme: light) {{
  :root:not([data-theme="dark"]) {{
    --bg-app: {light_bg_app};
    --bg-card: {light_bg_card};
    --bg-subtle: {light_bg_subtle};
    --border-color: {light_border};
    --text-main: {light_text};
    --text-dim: {light_text_dim};
    --text-muted: {light_text_muted};
    --primary: {light_primary};
    --primary-contrast: {light_primary_contrast};
    --primary-bg: {light_primary_bg};
    --accent: {light_accent};
    --success: {light_success};
    --success-bg: {light_success_bg};
    --warning: {light_warning};
    --warning-bg: {light_warning_bg};
    --danger: {light_danger};
    --danger-bg: {light_danger_bg};
    --info: {light_info};
    --info-bg: {light_info_bg};
  }}
}}

[data-theme="light"] {{
  --bg-app: {light_bg_app};
  --bg-card: {light_bg_card};
  --bg-subtle: {light_bg_subtle};
  --border-color: {light_border};
  --text-main: {light_text};
  --text-dim: {light_text_dim};
  --text-muted: {light_text_muted};
  --primary: {light_primary};
  --primary-contrast: {light_primary_contrast};
  --primary-bg: {light_primary_bg};
  --accent: {light_accent};
  --success: {light_success};
  --success-bg: {light_success_bg};
  --warning: {light_warning};
  --warning-bg: {light_warning_bg};
  --danger: {light_danger};
  --danger-bg: {light_danger_bg};
  --info: {light_info};
  --info-bg: {light_info_bg};
}}

[data-theme="dark"] {{
  --bg-app: {dark_bg_app};
  --bg-card: {dark_bg_card};
  --bg-subtle: {dark_bg_subtle};
  --border-color: {dark_border};
  --text-main: {dark_text};
  --text-dim: {dark_text_dim};
  --text-muted: {dark_text_muted};
  --primary: {dark_primary};
  --primary-contrast: {dark_primary_contrast};
  --primary-bg: {dark_primary_bg};
  --accent: {dark_accent};
  --success: {dark_success};
  --success-bg: {dark_success_bg};
  --warning: {dark_warning};
  --warning-bg: {dark_warning_bg};
  --danger: {dark_danger};
  --danger-bg: {dark_danger_bg};
  --info: {dark_info};
  --info-bg: {dark_info_bg};
}}


*, *::before, *::after {{
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}}

html, body {{
  background-color: var(--bg-app);
  color: var(--text-main);
  font-family: var(--font-sans);
  font-size: 14px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  min-height: 100vh;
  text-rendering: optimizeLegibility;
}}

/* --- Rendu tactile mobile & suppression des reflets parasites (Apple/Google) --- */
button, a, select, input {{
  touch-action: manipulation;
  -webkit-tap-highlight-color: transparent;
}}

/* --- Focus Ring Accessible W3C (WCAG AAA) --- */
:focus-visible {{
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}}

/* --- Respect des préférences de réduction de mouvement (Apple HIG & Google M3) --- */
@media (prefers-reduced-motion: reduce) {{
  *, *::before, *::after {{
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }}
}}
"#,
        dark_bg_app = DARK_THEME.bg_app,
        dark_bg_card = DARK_THEME.bg_card,
        dark_bg_subtle = DARK_THEME.bg_subtle,
        dark_border = DARK_THEME.border_color,
        dark_text = DARK_THEME.text_main,
        dark_text_dim = DARK_THEME.text_dim,
        dark_text_muted = DARK_THEME.text_muted,
        dark_primary = DARK_THEME.primary,
        dark_primary_contrast = DARK_THEME.primary_contrast,
        dark_primary_bg = DARK_THEME.primary_bg,
        dark_accent = DARK_THEME.accent,
        dark_success = DARK_THEME.success,
        dark_success_bg = DARK_THEME.success_bg,
        dark_warning = DARK_THEME.warning,
        dark_warning_bg = DARK_THEME.warning_bg,
        dark_danger = DARK_THEME.danger,
        dark_danger_bg = DARK_THEME.danger_bg,
        dark_info = DARK_THEME.info,
        dark_info_bg = DARK_THEME.info_bg,
        light_bg_app = LIGHT_THEME.bg_app,
        light_bg_card = LIGHT_THEME.bg_card,
        light_bg_subtle = LIGHT_THEME.bg_subtle,
        light_border = LIGHT_THEME.border_color,
        light_text = LIGHT_THEME.text_main,
        light_text_dim = LIGHT_THEME.text_dim,
        light_text_muted = LIGHT_THEME.text_muted,
        light_primary = LIGHT_THEME.primary,
        light_primary_contrast = LIGHT_THEME.primary_contrast,
        light_primary_bg = LIGHT_THEME.primary_bg,
        light_accent = LIGHT_THEME.accent,
        light_success = LIGHT_THEME.success,
        light_success_bg = LIGHT_THEME.success_bg,
        light_warning = LIGHT_THEME.warning,
        light_warning_bg = LIGHT_THEME.warning_bg,
        light_danger = LIGHT_THEME.danger,
        light_danger_bg = LIGHT_THEME.danger_bg,
        light_info = LIGHT_THEME.info,
        light_info_bg = LIGHT_THEME.info_bg,
        radius_sm = DARK_THEME.radius_sm,
        radius_md = DARK_THEME.radius_md,
        radius_lg = DARK_THEME.radius_lg,
        radius_pill = DARK_THEME.radius_pill,
        shadow_sm = DARK_THEME.shadow_sm,
        shadow_md = DARK_THEME.shadow_md,
    )
}

/// Génère la feuille de style globale complète vérifiée par Rust.
pub fn generate_global_style() -> Result<String, stylist::Error> {
    Ok(generate_global_tokens_css())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_global_style_output() {
        let res = generate_global_tokens_css();
        assert!(res.contains(":root {"));
        assert!(res.contains("[data-theme=\"light\"] {"));
        assert!(res.contains("--sat: env(safe-area-inset-top"));
        assert!(res.contains("--touch-target-min: 48px;"));
        assert!(res.contains(":focus-visible {"));
        assert!(res.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(res.contains("--bg-app: #0d1117;"));
        assert!(res.contains("--primary: #58a6ff;"));
    }
}
