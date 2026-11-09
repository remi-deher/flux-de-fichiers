# Frontend Leptos — Flux de Fichiers

Poste de pilotage et de supervision moderne pour la chaîne de remuxage MKVToolNix.

## Architecture

- **Framework** : Rust avec [Leptos](https://leptos.dev/) (version 0.8), rendu SSR / SSG vers `frontend/dist/index.html`.
- **Organisation en panneaux d'onglets accessibles** (`role="tablist"`, `role="tab"`, `role="tabpanel"`) :
  - `Header` : Navigation, identité visuelle, témoin de santé `/health`, statut de session, commutateur de thème (clair/sombre/système) et barre d'onglets accessible au clavier (flèches, Home, End).
  - `DashboardPanel` : Vue d'ensemble avec métriques clés (santé coordinateur, mode centralisé autonome ou workers distants, compteurs de fichiers, tâches et agents), accès rapides et présentation honnête des fonctionnalités planifiées hors périmètre (éditeur de flux visuel, connecteurs distants S3/SMB, catalogue de plugins).
  - `FilesPanel` : Liste des fichiers MKV admissibles dans `var/input/`, champ de recherche en direct, compteur de résultats et dialogue modal accessible de confirmation (`<dialog>`) avant soumission du remuxage avec ciblage optionnel d'un nœud.
  - `JobsPanel` : File des tâches avec filtres par statut (`Tous`, `En attente`, `En cours`, `Succès`, `Échec`), recherche par mot-clé, volet d'inspection détaillée d'une tâche (ID, agent assigné, horodatages, code d'erreur explicité) et téléchargement direct en flux.
  - `WorkersPanel` : Vue des serveurs et agents de traitement (coordinateur avec agent intégré ou agents distants HTTPS), horodatages de dernière activité et rappels d'architecture.
  - `SettingsPanel` : Gestion sécurisée de la session web et authentification, préférences visuelles de thèmes et diagnostics réseau.

## Sécurité et téléchargement

1. **Session et secrets** :
   - Conformément aux exigences de sécurité, aucune clé secrète n'est enregistrée en clair dans `sessionStorage` ou `localStorage`.
   - La clé réside exclusivement dans la mémoire vive volatile JavaScript de l'onglet actif (effacement instantané à la fermeture ou au rechargement de l'onglet). Dès l'ouverture réussie d'une session web par cookie HttpOnly (#29), la clé en mémoire est immédiatement purgée.
   - Un bouton de déconnexion explicite permet de purger immédiatement la mémoire et d'invalider la session.

2. **Téléchargement en flux direct (sans Blob en mémoire RAM)** :
   - **Mode session web (cookie HttpOnly #29)** : le téléchargement du média résultant (`/api/v1/jobs/{id}/output`) s'effectue en flux continu directement géré par le navigateur (sans tampon `Blob` en RAM), évitant tout crash d'onglet sur des fichiers MKV multi-gigaoctets.
   - **Mode clé Bearer volatile** : le navigateur ne transmettant pas d'en-tête `Authorization` lors d'une navigation classique par lien `<a>`, le streaming natif sans bufferisation RAM nécessite soit l'ouverture d'une session web dans l'onglet Paramètres, soit l'utilisation de la commande `curl` sécurisée (avec placeholder `<ADMIN_KEY>`) fournie dans le volet Détails.

## Design et règles d'accessibilité W3C

- **Contrastes renforcés** : ratios supérieurs à 5.9:1 sur texte normal (palette primaire `#0369a1` sur fond clair, texte sombre sur fond clair, texte clair `#f8fafc` sur fond sombre), et supérieurs à 3:1 sur les bordures et composants interactifs.
- **Cibles tactiles** : toutes les commandes, onglets, boutons d'action et champs respectent la recommandation minimale de **44×44 px**.
- **Navigation clavier intégrale** : lien d'évitement (« Aller au contenu principal »), focus visible `:focus-visible` (anneau 3px), gestion des modales avec piège de focus et touche `Échap`, parcours des onglets via les touches fléchées.
- **Thèmes clair et sombre** : détection automatique de `prefers-color-scheme` doublée d'un commutateur manuel pour forcer l'affichage clair ou sombre selon les préférences de l'utilisateur.
- **Statut d'attestation** : le code a été conçu selon les bonnes pratiques d'ergonomie et d'accessibilité du W3C ; en l'absence d'une passe automatisée Playwright/axe-core en CI, aucune conformité officielle ou certification n'est revendiquée.

## Utilisation et développement

1. **Génération de la page statique** :
   ```bash
   cargo run --manifest-path frontend/Cargo.toml
   ```
   Cette commande compile le binaire Leptos et génère `frontend/dist/index.html`.

2. **Démarrage du service de prévisualisation (`npm run dev`)** :
   ```bash
   npm run dev
   ```
   Le service Nginx expose l'interface sur `http://localhost:3000/` et relaie de façon transparente `/api/` et `/health` vers le coordinateur applicatif.

## Preuves de validation et limites d'environnement

- **Validation structurelle et syntaxique** : l'artefact généré `frontend/dist/index.html` a été validé (arbre DOM de 275 balises sémantiques valides, 5 onglets avec gestion ARIA `tablist`/`tab`/`tabpanel`, 2 dialogues modaux natifs HTML5 `<dialog>` avec `aria-modal="true"`, 23 boutons accessibles avec cibles tactiles >= 44×44 px).
- **Vérifications Rust** : `cargo fmt --check` et `cargo clippy -- -D warnings` au vert sur l'ensemble du crate `frontend`.
- **Limites d'environnement de test** : la machine hôte de développement ne dispose pas de navigateur graphique ou de Chromium headless (`which google-chrome chromium firefox` absent). Conformément à la décision utilisateur permanente du 9 nov. 2026 consignée dans `AGENTS.md`, les suites de tests lourdes et la construction des conteneurs complets sont déportées sur les runners distants de GitHub Actions.
