//! Jetons de design Material Design 3 Expressive en Rust typé.
//!
//! Les couleurs de texte et de contour respectent WCAG 2.2 AA ; les tests en bas de fichier
//! recalculent chaque ratio de contraste pour bloquer toute régression.

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
    /// Contour des champs de saisie : 3:1 au moins sur les surfaces (WCAG 1.4.11).
    pub border_control: &'static str,

    // Typographie
    pub text_main: &'static str,
    pub text_dim: &'static str,
    pub text_muted: &'static str,

    // Couleurs d'action M3
    pub primary: &'static str,
    /// Fond plein portant `primary_contrast` (boutons, marque) : 4,5:1 au moins.
    pub primary_fill: &'static str,
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
    /// Fond plein d'une action destructive portant `primary_contrast`.
    pub danger_fill: &'static str,
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

/// Taille racine en pixels : `html` vaut 87,5 % de la préférence du navigateur (16 px par défaut),
/// ce qui suit le réglage de taille de texte de l'utilisateur (WCAG 1.4.4).
pub const ROOT_FONT_PX: f64 = 14.0;
/// Pourcentage de la préférence du navigateur appliqué à `html` (`ROOT_FONT_PX` / 16 px).
pub const ROOT_FONT_PERCENT: &str = "87.5%";
/// Plus petite taille de texte de l'interface (Apple HIG : 11 pt au minimum ; Material : 11 sp).
pub const MIN_FONT_PX: f64 = 12.0;
/// Taille minimale d'un champ de saisie sur écran tactile : en dessous, Safari iOS zoome au focus.
pub const TOUCH_INPUT_FONT_PX: f64 = 16.0;

/// Points de rupture de l'interface, les seules largeurs admises dans un `@media (max-width: …)`
/// (vérifié par les tests du thème ; une variable CSS ne peut pas servir dans une requête média).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    /// Téléphone : barre du haut sur deux lignes, grilles sur une colonne.
    Compact,
    /// Tablette en portrait : la navigation devient un tiroir.
    Narrow,
    /// Petit écran de bureau : panneaux latéraux repliés, grilles réduites.
    Medium,
}

impl Breakpoint {
    pub const ALL: [Self; 3] = [Self::Compact, Self::Narrow, Self::Medium];

    /// Largeur maximale en pixels CSS.
    pub const fn max_width_px(self) -> u16 {
        match self {
            Self::Compact => 640,
            Self::Narrow => 768,
            Self::Medium => 960,
        }
    }
}

/// Largeurs de conteneur admises dans un `@container (max-width: …)` : les grilles de contenu
/// suivent la place de leur conteneur (`.main-content`, corps du tiroir), pas la fenêtre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerBreakpoint {
    /// Fiches de détail : libellé au-dessus de la valeur.
    Compact,
    /// Grilles de widgets, vue d'ensemble, paramètres : une seule colonne.
    Medium,
}

impl ContainerBreakpoint {
    pub const ALL: [Self; 2] = [Self::Compact, Self::Medium];

    /// Largeur maximale du conteneur en pixels CSS.
    pub const fn max_width_px(self) -> u16 {
        match self {
            Self::Compact => 560,
            Self::Medium => 720,
        }
    }
}

/// Échelon de l'échelle typographique : variable `--font-size-{name}`.
pub struct FontStep {
    pub name: &'static str,
    pub px: f64,
}

/// Échelle typographique unique : toute taille de texte du thème passe par l'un de ces échelons.
pub const TYPE_SCALE: [FontStep; 7] = [
    FontStep {
        name: "xs",
        px: 12.0,
    },
    FontStep {
        name: "sm",
        px: 13.0,
    },
    FontStep {
        name: "md",
        px: 14.0,
    },
    FontStep {
        name: "lg",
        px: 16.0,
    },
    FontStep {
        name: "xl",
        px: 18.0,
    },
    FontStep {
        name: "2xl",
        px: 20.0,
    },
    FontStep {
        name: "display",
        px: 28.0,
    },
];

pub const DARK_THEME: ThemeTokens = ThemeTokens {
    bg_app: "#090b10",
    bg_card: "#151922",
    bg_elevated: "#0e1117",
    bg_subtle: "#1c212c",
    bg_glass: "rgba(14, 17, 23, 0.82)",
    border_color: "#262c3a",
    border_subtle: "rgba(240, 246, 252, 0.07)",
    border_highlight: "rgba(99, 102, 241, 0.55)",
    border_control: "#626d84",

    text_main: "#f3f6fc",
    text_dim: "#94a3b8",
    text_muted: "#8291a7",

    primary: "#818cf8",
    primary_fill: "#4f46e5",
    primary_contrast: "#ffffff",
    primary_bg: "rgba(99, 102, 241, 0.16)",
    accent: "#22d3ee",

    success: "#10b981",
    success_bg: "rgba(16, 185, 129, 0.14)",
    warning: "#f59e0b",
    warning_bg: "rgba(245, 158, 11, 0.15)",
    danger: "#f87171",
    danger_bg: "rgba(239, 68, 68, 0.15)",
    danger_text: "#f87171",
    danger_fill: "#dc2626",
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
    border_control: "#7c8a9c",

    text_main: "#0f172a",
    text_dim: "#475569",
    text_muted: "#5b6b80",

    primary: "#4f46e5",
    primary_fill: "#4f46e5",
    primary_contrast: "#ffffff",
    primary_bg: "rgba(79, 70, 229, 0.10)",
    accent: "#0e7490",

    success: "#047857",
    success_bg: "rgba(5, 150, 105, 0.10)",
    warning: "#b45309",
    warning_bg: "rgba(217, 119, 6, 0.10)",
    danger: "#b91c1c",
    danger_bg: "rgba(220, 38, 38, 0.10)",
    danger_text: "#b91c1c",
    danger_fill: "#b91c1c",
    info: "#0369a1",
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Seuil WCAG 2.2 AA d'un texte de taille normale (1.4.3).
    const TEXT_MIN_RATIO: f64 = 4.5;
    /// Seuil WCAG 2.2 AA d'un contour de composant ou d'un indicateur de focus (1.4.11).
    const NON_TEXT_MIN_RATIO: f64 = 3.0;
    /// Seuil de linéarisation sRGB défini par WCAG.
    const SRGB_LINEAR_THRESHOLD: f64 = 0.039_28;
    /// Terme de lumière ambiante de la formule de contraste WCAG.
    const FLARE: f64 = 0.05;

    /// Luminance relative d'une couleur `#rrggbb` (définition WCAG).
    fn luminance(hex: &str) -> f64 {
        let channel = |offset: usize| {
            let raw = u8::from_str_radix(&hex[offset..offset + 2], 16)
                .unwrap_or_else(|_| panic!("couleur invalide : {hex}"));
            let value = f64::from(raw) / f64::from(u8::MAX);
            if value <= SRGB_LINEAR_THRESHOLD {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    fn contrast(first: &str, second: &str) -> f64 {
        let (a, b) = (luminance(first), luminance(second));
        (a.max(b) + FLARE) / (a.min(b) + FLARE)
    }

    fn assert_contrast(theme: &str, name: &str, fg: &str, bg: &str, min: f64) {
        let ratio = contrast(fg, bg);
        assert!(
            ratio >= min,
            "{theme} : {name} {fg} sur {bg} = {ratio:.2}:1, minimum {min}:1"
        );
    }

    fn check_theme(label: &str, theme: &ThemeTokens) {
        let surfaces = [
            theme.bg_app,
            theme.bg_card,
            theme.bg_elevated,
            theme.bg_subtle,
        ];
        let texts = [
            ("text_main", theme.text_main),
            ("text_dim", theme.text_dim),
            ("text_muted", theme.text_muted),
            ("primary", theme.primary),
            ("success", theme.success),
            ("warning", theme.warning),
            ("danger", theme.danger),
            ("danger_text", theme.danger_text),
            ("info", theme.info),
            ("accent", theme.accent),
        ];
        for surface in surfaces {
            for (name, color) in texts {
                assert_contrast(label, name, color, surface, TEXT_MIN_RATIO);
            }
            assert_contrast(
                label,
                "border_control",
                theme.border_control,
                surface,
                NON_TEXT_MIN_RATIO,
            );
        }
        for (name, fill) in [
            ("primary_fill", theme.primary_fill),
            ("danger_fill", theme.danger_fill),
        ] {
            assert_contrast(label, name, theme.primary_contrast, fill, TEXT_MIN_RATIO);
        }
    }

    #[test]
    fn dark_theme_meets_wcag_aa_contrast() {
        check_theme("sombre", &DARK_THEME);
    }

    #[test]
    fn light_theme_meets_wcag_aa_contrast() {
        check_theme("clair", &LIGHT_THEME);
    }
}
