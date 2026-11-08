# Flux de Fichiers

## Décisions permanentes

- Stack validée : Rust, Axum/Tokio, SQLx/PostgreSQL local, Docker, MKVToolNix.
  Leptos retenu mais différé ; aucune interface actuellement. Ne pas changer la
  stack sans demande utilisateur. Détails : `docs/adr/0001-stack.md`.
- Mode centralisé de 1 à N serveurs. HA optionnelle, bascule à cadrer ; aucun
  seuil de nœuds ne l'active. FFmpeg réservé aux médias synthétiques des tests.
- `npm run dev` doit démarrer sur http://localhost:3000 (bouton de prévisualisation).

## Sécurité

- TLS entre nœuds ; accès API authentifiés et autorisés.
- Aucune donnée sensible en clair en base ; erreurs et accès journalisés localement.
- Aucun secret versionné ou affiché. Secrets locaux dans `var/secrets/` ou `.env*`
  ignorés ; en déploiement, fournis par fichiers via l'orchestrateur.

## Travail

- Petits fichiers à responsabilité unique ; scinder avant de nuire à la lisibilité.
- Noms explicites, aucune constante numérique sans nom.
- Annoncer en deux lignes l'action prévue avant d'écrire du code.
- Lire seulement la rubrique utile de `docs/developpement.md` pour localiser le
  code, choisir les vérifications et retrouver les pièges. Ne pas charger toute
  la documentation au démarrage.
- Pour Git/CI/releases, lire `docs/adr/0002-flux-git.md` : flux `dev → test → main`,
  fusions par merge commit. Les exemples génériques des compétences ne décrivent
  pas nécessairement ce dépôt.
