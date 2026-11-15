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
    bg_app: "#090b10",
    bg_card: "#151922",
    bg_elevated: "#0e1117",
    bg_subtle: "#1c212c",
    bg_glass: "rgba(14, 17, 23, 0.82)",
    border_color: "#262c3a",
    border_subtle: "rgba(240, 246, 252, 0.07)",
    border_highlight: "rgba(99, 102, 241, 0.55)",

    text_main: "#f3f6fc",
    text_dim: "#94a3b8",
    text_muted: "#64748b",

    primary: "#6366f1",
    primary_contrast: "#ffffff",
    primary_bg: "rgba(99, 102, 241, 0.16)",
    accent: "#22d3ee",

    success: "#10b981",
    success_bg: "rgba(16, 185, 129, 0.14)",
    warning: "#f59e0b",
    warning_bg: "rgba(245, 158, 11, 0.15)",
    danger: "#ef4444",
    danger_bg: "rgba(239, 68, 68, 0.15)",
    danger_text: "#f87171",
    info: "#38bdf8",
    info_bg: "rgba(56, 189, 248, 0.15)",

    radius_sm: "6px",
    radius_md: "10px",
    radius_lg: "16px",
    radius_pill: "9999px",

    shadow_sm: "0 1px 2px rgba(0, 0, 0, 0.24)",
    shadow_md: "0 6px 18px rgba(0, 0, 0, 0.36)",
    shadow_lg: "0 16px 40px rgba(0, 0, 0, 0.55)",
    shadow_glow: "0 0 0 1px rgba(99, 102, 241, 0.35), 0 8px 24px rgba(99, 102, 241, 0.2)",
};

pub const LIGHT_THEME: ThemeTokens = ThemeTokens {
    bg_app: "#f1f5f9",
    bg_card: "#ffffff",
    bg_elevated: "#f8fafc",
    bg_subtle: "#f1f5f9",
    bg_glass: "rgba(248, 250, 252, 0.88)",
    border_color: "#e2e8f0",
    border_subtle: "rgba(15, 23, 42, 0.07)",
    border_highlight: "rgba(79, 70, 229, 0.45)",

    text_main: "#0f172a",
    text_dim: "#475569",
    text_muted: "#64748b",

    primary: "#4f46e5",
    primary_contrast: "#ffffff",
    primary_bg: "rgba(79, 70, 229, 0.10)",
    accent: "#0891b2",

    success: "#059669",
    success_bg: "rgba(5, 150, 105, 0.10)",
    warning: "#d97706",
    warning_bg: "rgba(217, 119, 6, 0.10)",
    danger: "#dc2626",
    danger_bg: "rgba(220, 38, 38, 0.10)",
    danger_text: "#b91c1c",
    info: "#0284c7",
    info_bg: "rgba(2, 132, 199, 0.10)",

    radius_sm: "6px",
    radius_md: "10px",
    radius_lg: "16px",
    radius_pill: "9999px",

    shadow_sm: "0 1px 2px rgba(0, 0, 0, 0.05)",
    shadow_md: "0 6px 16px rgba(15, 23, 42, 0.08)",
    shadow_lg: "0 16px 36px rgba(15, 23, 42, 0.12)",
    shadow_glow: "0 0 0 1px rgba(9, 105, 218, 0.25), 0 8px 20px rgba(9, 105, 218, 0.12)",
};
