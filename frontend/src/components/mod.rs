//! Composants actifs du poste de pilotage Flux de Fichiers.

pub mod dashboard_panel;
pub mod files_panel;
pub mod flows_panel;
pub mod header;
pub mod jobs_panel;
pub mod metrics_bar;
pub mod plugins_panel;
pub mod settings_panel;
pub mod workers_panel;

pub use dashboard_panel::DashboardPanel;
pub use files_panel::FilesPanel;
pub use flows_panel::FlowsPanel;
pub use header::Header;
pub use jobs_panel::JobsPanel;
pub use metrics_bar::MetricsBar;
pub use plugins_panel::PluginsPanel;
pub use settings_panel::SettingsPanel;
pub use workers_panel::WorkersPanel;
