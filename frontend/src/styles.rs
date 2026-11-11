//! Feuilles de styles et variables de design pour le poste de pilotage Flux de Fichiers.

pub const CSS_VARIABLES: &str = include_str!("../static/css/variables.css");
pub const CSS_BASE: &str = include_str!("../static/css/base.css");
pub const CSS_COMPONENTS: &str = include_str!("../static/css/components.css");
pub const CSS_PLUGINS: &str = include_str!("../static/css/plugins.css");
pub const CSS_FLOWS: &str = include_str!("../static/css/flows.css");

pub const CSS_STYLES: &str = concat!(
    include_str!("../static/css/variables.css"),
    "\n",
    include_str!("../static/css/base.css"),
    "\n",
    include_str!("../static/css/components.css"),
    "\n",
    include_str!("../static/css/plugins.css"),
    "\n",
    include_str!("../static/css/flows.css"),
);
