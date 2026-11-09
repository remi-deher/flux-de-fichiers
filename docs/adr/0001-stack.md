# 0001 — Rust, PostgreSQL et exécution Docker

Statut : accepté par l'utilisateur. Amendé le 9 novembre 2026.

Rust avec Axum/Tokio pour le coordinateur et les agents, SQLx et PostgreSQL pour
les métadonnées. L'interface Leptos, initialement différée lors du cadrage initial,
est mise en place dans `frontend/` suite à la décision utilisateur du 9 novembre 2026
afin de fournir l'interface de pilotage sur `localhost:3000`.

Tous les services s'exécutent sous Docker. Le remuxage sans réencodage repose sur
MKVToolNix, demandé explicitement en remplacement de FFmpeg pour cette opération.
FFmpeg ne sert qu'à fabriquer et analyser les fichiers synthétiques des tests.

Le mode est centralisé de 1 à N serveurs, avec un coordinateur et des agents. La
haute disponibilité reste optionnelle et nécessite un cadrage distinct ; aucun
seuil de nœuds ne l'active. Un laboratoire Compose sur un hôte ne constitue pas une
validation multi-hôte.
