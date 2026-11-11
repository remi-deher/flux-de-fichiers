//! Script client modulaire pour le poste de pilotage Flux de Fichiers.

pub const CLIENT_SCRIPT: &str = concat!(
    "(function() {\n  'use strict';\n\n",
    include_str!("../static/js/core.js"),
    "\n\n",
    include_str!("../static/js/auth.js"),
    "\n\n",
    include_str!("../static/js/tabs.js"),
    "\n\n",
    include_str!("../static/js/files.js"),
    "\n\n",
    include_str!("../static/js/jobs.js"),
    "\n\n",
    include_str!("../static/js/workers.js"),
    "\n\n",
    include_str!("../static/js/plugins.js"),
    "\n\n",
    include_str!("../static/js/flows.js"),
    "\n\n",
    include_str!("../static/js/init.js"),
    "\n})();\n"
);
