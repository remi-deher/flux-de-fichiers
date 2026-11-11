# 0001 — Rust, SQLite et exécution Docker

Statut : accepté par l'utilisateur. Amendé le 9 novembre 2026 et le 10 novembre 2026 (ADR 0005).

Rust avec Axum/Tokio pour le coordinateur et les agents. Les métadonnées et baux
sont gérés via SQLx avec SQLite embarqué dans `/config/flux.db` (en remplacement
de PostgreSQL acté par la décision du 10 novembre 2026, voir `docs/adr/0005-architecture-arr-conteneur-unique.md`).
L'interface Leptos dans `frontend/` est servie directement par Axum sur le port HTTP
unique `localhost:3000` avec repli SPA sur `index.html`.

Tous les services s'exécutent sous Docker (déploiement en conteneur unique style *arr).
Le remuxage sans réencodage repose sur MKVToolNix, demandé explicitement en remplacement
de FFmpeg pour cette opération. FFmpeg ne sert qu'à fabriquer et analyser les fichiers
synthétiques des tests.

Le mode est centralisé de 1 à N serveurs, avec un coordinateur et des agents. La
haute disponibilité reste optionnelle et nécessite un cadrage distinct ; aucun
seuil de nœuds ne l'active. Un laboratoire Compose sur un hôte ne constitue pas une
validation multi-hôte.
