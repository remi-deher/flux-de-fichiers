//! Générateur de styles globaux et design tokens CSS en Rust typé.

use super::tokens::{
    DARK_THEME, LIGHT_THEME, ROOT_FONT_PERCENT, ROOT_FONT_PX, TYPE_SCALE, ThemeTokens,
};

/// Déclarations `--font-size-*` de l'échelle typographique, en `rem` (suivent la racine).
fn type_scale_css() -> String {
    TYPE_SCALE
        .iter()
        .map(|step| {
            format!(
                "  --font-size-{}: {}rem;\n",
                step.name,
                format_rem(step.px / ROOT_FONT_PX)
            )
        })
        .collect()
}

/// Valeur `rem` sur six décimales au plus, sans zéros inutiles.
fn format_rem(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Déclarations de variables CSS pour un jeu de jetons (clair ou sombre).
fn theme_vars_css(theme: &ThemeTokens) -> String {
    format!(
        r#"  --bg-app: {bg_app};
  --bg-card: {bg_card};
  --bg-elevated: {bg_elevated};
  --bg-subtle: {bg_subtle};
  --bg-glass: {bg_glass};
  --border-color: {border_color};
  --border-subtle: {border_subtle};
  --border-highlight: {border_highlight};
  --border-control: {border_control};
  --text-main: {text_main};
  --text-dim: {text_dim};
  --text-muted: {text_muted};
  --primary: {primary};
  --primary-fill: {primary_fill};
  --primary-contrast: {primary_contrast};
  --primary-bg: {primary_bg};
  --accent: {accent};
  --success: {success};
  --success-bg: {success_bg};
  --warning: {warning};
  --warning-bg: {warning_bg};
  --danger: {danger};
  --danger-bg: {danger_bg};
  --danger-text: {danger_text};
  --danger-fill: {danger_fill};
  --info: {info};
  --info-bg: {info_bg};
  --shadow-sm: {shadow_sm};
  --shadow-md: {shadow_md};
  --shadow-lg: {shadow_lg};
  --shadow-glow: {shadow_glow};"#,
        bg_app = theme.bg_app,
        bg_card = theme.bg_card,
        bg_elevated = theme.bg_elevated,
        bg_subtle = theme.bg_subtle,
        bg_glass = theme.bg_glass,
        border_color = theme.border_color,
        border_subtle = theme.border_subtle,
        border_highlight = theme.border_highlight,
        border_control = theme.border_control,
        text_main = theme.text_main,
        text_dim = theme.text_dim,
        text_muted = theme.text_muted,
        primary = theme.primary,
        primary_fill = theme.primary_fill,
        primary_contrast = theme.primary_contrast,
        primary_bg = theme.primary_bg,
        accent = theme.accent,
        success = theme.success,
        success_bg = theme.success_bg,
        warning = theme.warning,
        warning_bg = theme.warning_bg,
        danger = theme.danger,
        danger_bg = theme.danger_bg,
        danger_text = theme.danger_text,
        danger_fill = theme.danger_fill,
        info = theme.info,
        info_bg = theme.info_bg,
        shadow_sm = theme.shadow_sm,
        shadow_md = theme.shadow_md,
        shadow_lg = theme.shadow_lg,
        shadow_glow = theme.shadow_glow,
    )
}

/// Génère la feuille de style globale contenant les variables de design tokens et les resets racine.
pub fn generate_global_tokens_css() -> String {
    let dark_vars = theme_vars_css(&DARK_THEME);
    let light_vars = theme_vars_css(&LIGHT_THEME);
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
  /* Plancher WCAG 2.5.8 d'une cible au pointeur fin, et taille des cases à cocher. */
  --touch-target-pointer: 24px;
  --control-check-size: 16px;

  /* --- Échelle d'espacement (mise en page aérée) --- */
  --space-1: 0.5rem;
  --space-2: 1rem;
  --space-3: 1.5rem;
  --space-4: 2rem;

  /* --- Échelle typographique (12 px au minimum) --- */
{type_scale}
  /* --- Rayons, courbes et typographie --- */
  --radius-sm: {radius_sm};
  --radius-md: {radius_md};
  --radius-lg: {radius_lg};
  --radius-pill: {radius_pill};
  --ease-out: cubic-bezier(0.16, 1, 0.3, 1);
  --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji";
  --font-mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;

  /* --- Surfaces & Couleurs Thème Sombre --- */
{dark_vars}
}}

@media (prefers-color-scheme: light) {{
  :root:not([data-theme="dark"]) {{
{light_vars}
  }}
}}

[data-theme="light"] {{
{light_vars}
}}

[data-theme="dark"] {{
{dark_vars}
}}


*, *::before, *::after {{
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}}

html {{
  font-size: {root_percent};
}}

html, body {{
  background-color: var(--bg-app);
  color: var(--text-main);
  font-family: var(--font-sans);
  line-height: 1.5;
  font-variant-numeric: tabular-nums;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  min-height: 100vh;
  text-rendering: optimizeLegibility;
}}

/* Le corps prend sa taille de l'échelle ; `html` garde la racine en pourcentage (sinon `1rem`
   y vaudrait la préférence du navigateur entière). */
body {{
  font-size: var(--font-size-md);
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
        type_scale = type_scale_css(),
        root_percent = ROOT_FONT_PERCENT,
        radius_sm = DARK_THEME.radius_sm,
        radius_md = DARK_THEME.radius_md,
        radius_lg = DARK_THEME.radius_lg,
        radius_pill = DARK_THEME.radius_pill,
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
        assert!(res.contains("--space-3: 1.5rem;"));
        assert!(res.contains(":focus-visible {"));
        assert!(res.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(res.contains("--bg-app: #151416;"));
        assert!(res.contains("--bg-elevated: #1c1b1e;"));
        assert!(res.contains("--shadow-glow:"));
        assert!(res.contains("--ease-out:"));
        assert!(res.contains("--font-size-xs: 0.857143rem;"));
        assert!(res.contains("--font-size-md: 1rem;"));
        assert!(res.contains("font-size: 87.5%;"));
        assert!(res.contains("--touch-target-pointer: 24px;"));
        assert!(res.contains("--primary: #f0ad80;"));
        assert!(res.contains("--primary-fill: #994823;"));
        assert!(res.contains("--danger-fill: #dc2626;"));
        assert!(res.contains("--border-control: #817b83;"));
        assert!(res.contains("--danger-text: #f87171;"));
        assert!(res.contains("--danger-text: #b91c1c;"));
        assert!(res.contains("font-variant-numeric: tabular-nums;"));
    }
}
