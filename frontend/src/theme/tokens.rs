//! Jetons de design Material Design 3 Expressive en Rust typé.

pub struct ThemeTokens {
    // Surfaces
    pub bg_app: &'static str,
    pub bg_card: &'static str,
    pub bg_elevated: &'static str,
    pub bg_subtle: &'static str,
    pub bg_glass: &'static str,
    pub border_color: &'static str,
    pub border_subtle: &'static str,
    pub border_highlight: &'static str,

    // Typographie
    pub text_main: &'static str,
    pub text_dim: &'static str,
    pub text_muted: &'static str,

    // Couleurs d'action M3
    pub primary: &'static str,
    pub primary_contrast: &'static str,
    pub primary_bg: &'static str,
    pub accent: &'static str,

    // Statuts sémantiques
    pub success: &'static str,
    pub success_bg: &'static str,
    pub warning: &'static str,
    pub warning_bg: &'static str,
    pub danger: &'static str,
    pub danger_bg: &'static str,
    pub danger_text: &'static str,
    pub info: &'static str,
    pub info_bg: &'static str,

    // Rayons
    pub radius_sm: &'static str,
    pub radius_md: &'static str,
    pub radius_lg: &'static str,
    pub radius_pill: &'static str,

    // Ombres
    pub shadow_sm: &'static str,
    pub shadow_md: &'static str,
    pub shadow_lg: &'static str,
    pub shadow_glow: &'static str,
}

pub const DARK_THEME: ThemeTokens = ThemeTokens {
    bg_app: "#0d1117",
    bg_card: "#161b22",
    bg_elevated: "#1c2129",
    bg_subtle: "#21262d",
    bg_glass: "rgba(22, 27, 34, 0.78)",
    border_color: "#30363d",
    border_subtle: "rgba(240, 246, 252, 0.08)",
    border_highlight: "rgba(88, 166, 255, 0.45)",

    text_main: "#f0f6fc",
    text_dim: "#8b949e",
    text_muted: "#6e7681",

    primary: "#58a6ff",
    primary_contrast: "#ffffff",
    primary_bg: "rgba(56, 139, 253, 0.15)",
    accent: "#a371f7",

    success: "#3fb950",
    success_bg: "rgba(46, 160, 67, 0.15)",
    warning: "#d29922",
    warning_bg: "rgba(187, 128, 9, 0.15)",
    danger: "#f85149",
    danger_bg: "rgba(248, 81, 73, 0.15)",
    danger_text: "#ff7b72",
    info: "#58a6ff",
    info_bg: "rgba(56, 139, 253, 0.15)",

    radius_sm: "6px",
    radius_md: "10px",
    radius_lg: "16px",
    radius_pill: "9999px",

    shadow_sm: "0 1px 2px rgba(0, 0, 0, 0.24)",
    shadow_md: "0 6px 18px rgba(0, 0, 0, 0.36)",
    shadow_lg: "0 16px 40px rgba(0, 0, 0, 0.55)",
    shadow_glow: "0 0 0 1px rgba(88, 166, 255, 0.3), 0 8px 24px rgba(56, 139, 253, 0.18)",
};

pub const LIGHT_THEME: ThemeTokens = ThemeTokens {
    bg_app: "#f6f8fa",
    bg_card: "#ffffff",
    bg_elevated: "#ffffff",
    bg_subtle: "#f3f4f6",
    bg_glass: "rgba(255, 255, 255, 0.85)",
    border_color: "#d0d7de",
    border_subtle: "rgba(31, 35, 40, 0.08)",
    border_highlight: "rgba(9, 105, 218, 0.4)",

    text_main: "#1f2328",
    text_dim: "#656d76",
    text_muted: "#8c959f",

    primary: "#0969da",
    primary_contrast: "#ffffff",
    primary_bg: "rgba(9, 105, 218, 0.1)",
    accent: "#8250df",

    success: "#1a7f37",
    success_bg: "rgba(26, 127, 55, 0.1)",
    warning: "#9a6700",
    warning_bg: "rgba(154, 103, 0, 0.1)",
    danger: "#cf222e",
    danger_bg: "rgba(207, 34, 46, 0.1)",
    danger_text: "#a40e26",
    info: "#0969da",
    info_bg: "rgba(9, 105, 218, 0.1)",

    radius_sm: "6px",
    radius_md: "10px",
    radius_lg: "16px",
    radius_pill: "9999px",

    shadow_sm: "0 1px 2px rgba(0, 0, 0, 0.05)",
    shadow_md: "0 6px 16px rgba(15, 23, 42, 0.08)",
    shadow_lg: "0 16px 36px rgba(15, 23, 42, 0.12)",
    shadow_glow: "0 0 0 1px rgba(9, 105, 218, 0.25), 0 8px 20px rgba(9, 105, 218, 0.12)",
};
