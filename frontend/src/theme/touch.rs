//! Cibles et saisie tactiles : tailles minimales des contrôles (WCAG 2.5.8, Apple HIG 44 pt,
//! Material 48 dp) et champs à 16 px sur écran tactile pour éviter le zoom de Safari iOS.
//! Chargé en dernier : ces planchers l'emportent sur les tailles compactes des composants.

use super::tokens::TOUCH_INPUT_FONT_PX;

/// Feuille des cibles tactiles, générée pour y injecter les constantes typées.
pub fn touch_css() -> String {
    format!(
        r#"
/* Cases à cocher et boutons radio : 16 px, cible de 24 px avec leur espacement (WCAG 2.5.8). */
input[type="checkbox"],
input[type="radio"] {{
  width: var(--control-check-size);
  height: var(--control-check-size);
  accent-color: var(--primary-fill);
}}

.theme-segmented__btn {{
  min-height: var(--touch-target-pointer);
}}

/* Pointeur grossier (doigt) : 44 px au moins pour toute cible, 16 px pour toute saisie. */
@media (pointer: coarse) {{
  input:not([type="checkbox"]):not([type="radio"]),
  select,
  textarea {{
    font-size: max({input_px}px, var(--font-size-lg));
  }}

  .btn,
  .tab-btn,
  .form-input,
  .form-select,
  .topbar-search,
  .theme-segmented__btn,
  .sidebar-collapse-btn,
  .inspection-drawer__tab {{
    min-height: var(--touch-target-compact);
  }}

  .sidebar-logout-btn,
  .topbar-menu-btn,
  .topbar-search-clear,
  .toast-item__close,
  .header-icon-btn {{
    min-width: var(--touch-target-compact);
    min-height: var(--touch-target-compact);
  }}

  input[type="checkbox"],
  input[type="radio"] {{
    width: var(--touch-target-pointer);
    height: var(--touch-target-pointer);
  }}
}}

/* Sans survol (écran tactile seul) : pas de raccourcis clavier affichés. */
@media (hover: none) {{
  .kbd-shortcut {{
    display: none;
  }}
}}
"#,
        input_px = TOUCH_INPUT_FONT_PX,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_inputs_never_fall_below_sixteen_pixels() {
        let css = touch_css();
        assert!(css.contains("@media (pointer: coarse)"));
        assert!(css.contains("font-size: max(16px, var(--font-size-lg));"));
        assert!(css.contains("min-height: var(--touch-target-compact);"));
        assert!(css.contains("@media (hover: none)"));
    }
}
