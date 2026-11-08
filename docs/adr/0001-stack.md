# 0001 — Rust, PostgreSQL et exécution Docker

Statut : accepté par l'utilisateur.

Rust avec Axum/Tokio pour le coordinateur et les agents, SQLx et PostgreSQL pour
les métadonnées. Leptos est retenu pour une interface future mais son implémentation
est différée à la demande de l'utilisateur. Le périmètre présent est la validation
de la chaîne et des scénarios à un ou plusieurs nœuds.

Tous les services s'exécutent sous Docker. Le remuxage sans réencodage repose sur
MKVToolNix, demandé explicitement en remplacement de FFmpeg pour cette opération.
FFmpeg ne sert qu'à fabriquer et analyser les fichiers synthétiques des tests.

Le mode est centralisé de 1 à N serveurs, avec un coordinateur et des agents. La
haute disponibilité reste optionnelle et nécessite un cadrage distinct ; aucun
seuil de nœuds ne l'active. Un laboratoire Compose sur un hôte ne constitue pas une
validation multi-hôte.
