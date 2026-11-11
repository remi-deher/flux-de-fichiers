# Flux de Fichiers

## Décisions permanentes

- Stack validée : Rust, Axum/Tokio, SQLx/SQLite local (`/config/flux.db`), Docker (conteneur
  unique style *arr), MKVToolNix, et interface Leptos servie directement par Axum
  (décision utilisateur du 10 nov. 2026). Ne pas changer la stack sans demande
  utilisateur. Détails : `docs/adr/0001-stack.md` et `docs/adr/0005-architecture-arr-conteneur-unique.md`.
- Mode centralisé de 1 à N serveurs. HA optionnelle, bascule à cadrer ; aucun
  seuil de nœuds ne l'active. FFmpeg réservé aux médias synthétiques des tests.
- `npm run dev` doit démarrer sur http://localhost:3000 (bouton de prévisualisation).
- Builds et tests à distance : ne pas initialiser ni exécuter de builds Docker lourds
  ou de suites de tests en local (faible puissance de la machine hôte, décision utilisateur
  du 9 nov. 2026). Les tests et la construction des images s'exécutent à distance sur
  GitHub CI, et les images sont tirées directement pour la production.

## Sécurité

- Port HTTP unique (3000 par défaut, TLS optionnel via reverse proxy externe) ; accès
  API authentifiés et autorisés par clés (admin / worker) avec auto-amorçage dans `/config`.
- Aucune donnée sensible en clair en base ; erreurs et accès journalisés localement.
- Aucun secret versionné ou affiché. Secrets locaux dans `var/secrets/` ou `.env*`
  ignorés ; en déploiement, fournis par fichiers via l'orchestrateur.

## Travail

- Petits fichiers à responsabilité unique ; scinder avant de nuire à la lisibilité.
- Noms explicites, aucune constante numérique sans nom.
- Tester systématiquement toutes les fonctions et routes du code avec des tests unitaires ou d'intégration Rust natifs légers pour éviter toute régression non prévue (décision utilisateur du 12 nov. 2026).
- Annoncer en deux lignes l'action prévue avant d'écrire du code.
- Lire seulement la rubrique utile de `docs/developpement.md` pour localiser le
  code, choisir les vérifications et retrouver les pièges. Ne pas charger toute
  la documentation au démarrage.
- Pour Git/CI/releases, lire `docs/adr/0002-flux-git.md` : flux `dev → test → main`,
  fusions par merge commit. Les exemples génériques des compétences ne décrivent
  pas nécessairement ce dépôt.
