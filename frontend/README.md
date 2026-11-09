# Frontend Leptos — Flux de Fichiers

Interface de pilotage et de supervision pour la chaîne de remuxage MKVToolNix.

## Architecture

- **Framework** : Rust avec [Leptos](https://leptos.dev/) (version 0.8), rendu SSR / SSG vers `frontend/dist/index.html`.
- **Composants** :
  - `Header` : Navigation, identité visuelle, témoin de disponibilité `/health` et bouton d'actualisation.
  - `MetricsBar` : Indicateurs opérationnels (coordinateur joignable, agents enregistrés, fichiers sources, file de tâches en base).
  - `AuthCard` : Gestion locale de la clé administrateur (`ADMIN_KEY`), confinée à la session locale du navigateur (`sessionStorage`).
  - `FilesTable` : Liste des fichiers MKV admissibles dans `INPUT_DIR` et déclenchement de remuxage (`POST /api/v1/jobs`).
  - `JobsTable` : Suivi des tâches selon les statuts de l'API (`queued`, `running`, `succeeded`, `failed`) et téléchargement authentifié du résultat (`GET /api/v1/jobs/{id}/output`).
  - `WorkersTable` : Liste des agents de remuxage enregistrés et date de dernier contact (`last_seen`).
  - `FutureSection` : Annonce honnête des fonctionnalités hors périmètre (éditeur visuel de flux, connecteurs distants S3/SMB).
- **Design & Accessibilité** :
  - Conçue selon les recommandations d'accessibilité (contrastes renforcés >= 4.5:1 sur texte et >= 3:1 sur composants, navigation complète au clavier, repères sémantiques W3C).
  - Lien d'évitement (« Aller au contenu principal »), focus visible `:focus-visible`, labels explicites associés aux champs.
  - Cibles tactiles >= 44x44 px, mise en page adaptative ordinateur et mobile sans débordement horizontal.
  - Respect de `prefers-reduced-motion` et `prefers-color-scheme` (mode clair et sombre).

## Utilisation et développement

1. **Génération de la page statique** :
   ```bash
   cargo run --manifest-path frontend/Cargo.toml
   ```
   Cette commande exécute le binaire SSR Leptos et produit `frontend/dist/index.html`.

2. **Lancement via Compose (`npm run dev`)** :
   ```bash
   npm run dev
   ```
   Le service de prévisualisation Nginx sert l'interface sur `http://localhost:3000/` tout en relayant les requêtes API (`/api/` et `/health`) vers le coordinateur TLS (`https://coordinator:3443`).

## Limites actuelles et fonctionnement

- **Authentification du téléchargement** : Le téléchargement du résultat (`/api/v1/jobs/{id}/output`) transmet le jeton `Authorization: Bearer` via `fetch` puis assemble le fichier en mémoire du navigateur (`Blob`). Pour des médias de plusieurs gigaoctets, cette méthode est limitée par la mémoire vive allouée à l'onglet : il est recommandé d'utiliser `curl` en ligne de commande avec le jeton Bearer ou d'accéder directement au volume Docker de données.
- **Interrogation et statuts** : L'interface repose sur une interrogation périodique (toutes les 30 s pour `/health`) ou manuelle via le bouton d'actualisation, sans connexion WebSocket permanente.
- **Périmètre applicatif** : L'éditeur de flux et les connecteurs vers des stockages distants (S3, SMB) sont hors du périmètre présent.
