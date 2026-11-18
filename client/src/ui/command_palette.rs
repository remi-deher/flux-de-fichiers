//! Palette de commandes clavier (`Ctrl+K` / `Cmd+K`) pour naviguer et piloter l'interface.

use super::icon::{Icon, IconName};
use super::search::matches;
use super::toast::Toasts;
use crate::ai::AiAssistantState;
use crate::route::{Route, use_router};
use crate::shell::{allowed, groups};
use crate::theme::{Theme, set_theme};
use flux_shared::InstanceKind;
use flux_shared::session::Me;
use leptos::prelude::*;

const HEADER_ICON_SIZE: u32 = 16;
const ITEM_ICON_SIZE: u32 = 15;
const ARROW_ICON_SIZE: u32 = 14;

/// Entrée exécutable depuis la palette de commandes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandItem {
    pub id: String,
    pub group: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub icon: IconName,
    pub route: Option<Route>,
    pub theme: Option<Theme>,
    pub open_ai: bool,
}

fn theme_label(theme: Theme) -> &'static str {
    match theme {
        Theme::System => "Thème automatique",
        Theme::Light => "Thème clair",
        Theme::Dark => "Thème sombre",
    }
}

/// Commandes accessibles pour cet utilisateur sur cette installation.
pub fn available_commands(me: &Me) -> Vec<CommandItem> {
    let mut commands = Vec::new();
    for (group_name, screens) in groups() {
        for screen in screens {
            if allowed(me, &screen.route) {
                commands.push(CommandItem {
                    id: format!("nav:{}", screen.route.path()),
                    group: "Navigation",
                    label: screen.label,
                    hint: group_name,
                    icon: screen.icon,
                    route: Some(screen.route),
                    theme: None,
                    open_ai: false,
                });
            }
        }
    }
    if allowed(me, &Route::NewFlow) {
        commands.push(CommandItem {
            id: "action:new-flow".to_owned(),
            group: "Actions",
            label: "Créer un nouveau flux",
            hint: "Éditeur nodal",
            icon: IconName::Plus,
            route: Some(Route::NewFlow),
            theme: None,
            open_ai: false,
        });
    }
    if me.instance_kind == InstanceKind::Central {
        commands.push(CommandItem {
            id: "action:assistant-ai".to_owned(),
            group: "Actions",
            label: "Ouvrir l'assistant IA",
            hint: "Copilote et diagnostic MCP",
            icon: IconName::Sparkles,
            route: None,
            theme: None,
            open_ai: true,
        });
    }
    for theme in Theme::ALL {
        commands.push(CommandItem {
            id: format!("theme:{}", theme.as_str()),
            group: "Apparence",
            label: theme_label(theme),
            hint: "Basculer le thème",
            icon: IconName::Sliders,
            route: None,
            theme: Some(theme),
            open_ai: false,
        });
    }
    commands
}

/// Filtre les commandes selon la saisie (libellé, groupe ou indice).
pub fn filter_commands(commands: &[CommandItem], query: &str) -> Vec<CommandItem> {
    commands
        .iter()
        .filter(|cmd| matches(&[cmd.label, cmd.group, cmd.hint], query))
        .cloned()
        .collect()
}

/// Déplace l'indice sélectionné de façon circulaire dans `[0, total)`.
pub fn move_selection(current: usize, total: usize, delta: isize) -> usize {
    if total == 0 {
        return 0;
    }
    let clamped = current.min(total - 1) as isize;
    (clamped + delta).rem_euclid(total as isize) as usize
}

/// État d'ouverture partagé entre la barre du haut et la palette.
#[derive(Clone, Copy)]
pub struct CommandPaletteState {
    pub open: RwSignal<bool>,
}

impl CommandPaletteState {
    pub fn install() -> Self {
        let state = Self {
            open: RwSignal::new(false),
        };
        provide_context(state);
        state
    }

    pub fn show(self) {
        self.open.set(true);
    }

    pub fn hide(self) {
        self.open.set(false);
    }

    pub fn toggle(self) {
        self.open.update(|open| *open = !*open);
    }
}

/// Boîte de dialogue modale `Ctrl+K` / `Cmd+K`.
#[component]
pub fn CommandPalette(me: Me) -> impl IntoView {
    let state = use_context::<CommandPaletteState>().unwrap_or_else(CommandPaletteState::install);
    let ai_state = use_context::<AiAssistantState>();
    let router = use_router();
    let all_commands = StoredValue::new(available_commands(&me));
    let query = RwSignal::new(String::new());
    let active_index = RwSignal::new(0_usize);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    let filtered =
        Memo::new(move |_| all_commands.with_value(|cmds| filter_commands(cmds, &query.get())));

    Effect::new(move |_| {
        if state.open.get() {
            query.set(String::new());
            active_index.set(0);
            if let Some(input) = input_ref.get() {
                let _ = input.focus();
            }
        }
    });

    let _shortcut = window_event_listener(leptos::ev::keydown, move |event| {
        let key = event.key();
        if (event.ctrl_key() || event.meta_key()) && key.eq_ignore_ascii_case("k") {
            event.prevent_default();
            state.toggle();
        } else if key == "Escape" && state.open.get_untracked() {
            state.hide();
        }
    });

    let run_command = move |cmd: CommandItem| {
        state.hide();
        if let Some(route) = cmd.route {
            router.go(route);
        }
        if let Some(theme) = cmd.theme {
            set_theme(theme);
            Toasts::notify_info(format!("{} activé.", cmd.label));
        }
        if cmd.open_ai
            && let Some(ai) = ai_state
        {
            ai.show();
        }
    };

    view! {
        <Show when=move || state.open.get()>
            <div class="cmd-palette" role="dialog" aria-modal="true" aria-label="Palette de commandes">
                <button type="button" class="cmd-palette__scrim" aria-label="Fermer la palette"
                    on:click=move |_| state.hide()></button>
                <div class="cmd-palette__dialog">
                    <div class="cmd-palette__header">
                        <Icon name=IconName::Command size=HEADER_ICON_SIZE />
                        <input
                            node_ref=input_ref
                            type="search"
                            class="cmd-palette__input"
                            placeholder="Aller à un écran, créer un flux ou changer de thème…"
                            aria-label="Rechercher une commande"
                            autocomplete="off"
                            spellcheck="false"
                            prop:value=move || query.get()
                            on:input=move |event| {
                                query.set(event_target_value(&event));
                                active_index.set(0);
                            }
                            on:keydown=move |event| {
                                let count = filtered.with_untracked(Vec::len);
                                match event.key().as_str() {
                                    "ArrowDown" => {
                                        event.prevent_default();
                                        active_index.update(|idx| *idx = move_selection(*idx, count, 1));
                                    }
                                    "ArrowUp" => {
                                        event.prevent_default();
                                        active_index.update(|idx| *idx = move_selection(*idx, count, -1));
                                    }
                                    "Enter" => {
                                        event.prevent_default();
                                        let idx = active_index.get_untracked();
                                        if let Some(cmd) = filtered.with_untracked(|list| list.get(idx).cloned()) {
                                            run_command(cmd);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        />
                        <span class="kbd-shortcut">"Échap"</span>
                    </div>
                    <div class="cmd-palette__list" role="listbox" aria-label="Commandes disponibles">
                        {move || {
                            let items = filtered.get();
                            if items.is_empty() {
                                return view! {
                                    <div class="cmd-palette__empty">"Aucune commande ne correspond à cette recherche."</div>
                                }
                                .into_any();
                            }
                            items.into_iter().enumerate().map(|(index, cmd)| {
                                let target = cmd.clone();
                                view! {
                                    <button
                                        type="button"
                                        role="option"
                                        class="cmd-palette__item"
                                        class:is-active=move || active_index.get() == index
                                        aria-selected=move || (active_index.get() == index).to_string()
                                        on:mouseenter=move |_| active_index.set(index)
                                        on:click=move |_| run_command(target.clone())
                                    >
                                        <span class="cmd-palette__item-main">
                                            <Icon name=cmd.icon size=ITEM_ICON_SIZE />
                                            <span class="cmd-palette__label">{cmd.label}</span>
                                            <span class="cmd-palette__hint">{cmd.hint}</span>
                                        </span>
                                        <span class="cluster">
                                            <span class="cmd-palette__badge">{cmd.group}</span>
                                            <Icon name=IconName::ChevronRight size=ARROW_ICON_SIZE />
                                        </span>
                                    </button>
                                }
                            }).collect_view().into_any()
                        }}
                    </div>
                    <div class="cmd-palette__footer">
                        <span>"↑ ↓ naviguer • Entrée exécuter"</span>
                        <span>"Ctrl+K pour basculer"</span>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::session::CallerKind;
    use flux_shared::{InstanceKind, Role};

    fn me(role: Role, instance_kind: InstanceKind) -> Me {
        Me::new(
            CallerKind::Account,
            "remi".into(),
            role,
            instance_kind,
            None,
        )
    }

    #[test]
    fn available_commands_respect_instance_and_permissions() {
        let admin = me(Role::Admin, InstanceKind::Central);
        let admin_cmds = available_commands(&admin);
        assert!(
            admin_cmds
                .iter()
                .any(|cmd| cmd.route == Some(Route::NewFlow))
        );
        assert!(
            admin_cmds
                .iter()
                .any(|cmd| cmd.route == Some(Route::Libraries))
        );

        assert!(admin_cmds.iter().any(|cmd| cmd.open_ai));

        let worker = me(Role::Admin, InstanceKind::Worker);
        let worker_cmds = available_commands(&worker);
        assert!(!worker_cmds.iter().any(|cmd| cmd.open_ai));
        assert!(
            !worker_cmds
                .iter()
                .any(|cmd| cmd.route == Some(Route::NewFlow))
        );
        assert!(
            !worker_cmds
                .iter()
                .any(|cmd| cmd.route == Some(Route::Libraries))
        );
        assert!(
            worker_cmds
                .iter()
                .any(|cmd| cmd.route == Some(Route::Overview))
        );

        let viewer = me(Role::Viewer, InstanceKind::Central);
        let viewer_cmds = available_commands(&viewer);
        assert!(
            !viewer_cmds
                .iter()
                .any(|cmd| cmd.route == Some(Route::NewFlow))
        );
    }

    #[test]
    fn filtering_and_circular_selection_work_safely() {
        let admin = me(Role::Admin, InstanceKind::Central);
        let all = available_commands(&admin);
        let themes = filter_commands(&all, "sombre");
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].theme, Some(Theme::Dark));

        assert_eq!(move_selection(0, 0, 1), 0);
        assert_eq!(move_selection(0, 3, -1), 2);
        assert_eq!(move_selection(2, 3, 1), 0);
        assert_eq!(move_selection(9, 3, 1), 0);
    }
}
