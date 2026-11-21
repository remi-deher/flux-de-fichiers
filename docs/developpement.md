# Développement : lire selon la tâche

`AGENTS.md` contient les règles permanentes ; `CLAUDE.md` l'importe sans copie.
État du chantier, points non vérifiés et suite : [`reprise.md`](reprise.md).
Ce guide se consulte à la demande. Le README reste la référence d'utilisation
(démarrage, API, scénarios, déploiement), les ADR celle des décisions.

## Points d'entrée

| Travail | Lire d'abord | Compléter si nécessaire |
|---|---|---|
| API, authentification | `src/api.rs`, `src/auth.rs`, `src/api/jobs.rs` | Route concernée dans `docs/openapi.yaml`, `tests/api_security.rs` |
| Baux, attribution, reprises | `src/repository.rs`, `src/domain.rs` | `migrations/`, `src/worker.rs` |
| Transferts, intégrité, publication | `src/api/transfers.rs`, `src/storage.rs` | `src/media.rs`, `src/worker.rs` |
| Variables de script, exposition par les plugins (ADR 0025) | `shared/src/script_variables.rs`, `shared/src/exposed_variables.rs`, `src/flows/variables.rs`, `src/flows/node_references.rs`, `shared/src/flow_variables.rs`, `client/src/flows/variables_form.rs`, `src/plugins/script_requirements.rs`, `src/plugins/exposures.rs`, `client/src/plugins/exposed_variables.rs`, `client/src/libraries/requirements_editor.rs`, `client/src/libraries/rows.rs`, `src/plugins/local/write.rs`, `client/src/libraries/variables_editor.rs`, `client/src/ui/variables_table.rs` | `docs/adr/0025-variables-de-script-et-exposition-par-les-plugins.md` ; tests : `cargo test -p forgearr-shared -p forgearr --lib script_variables` |
| Aperçu et coloration du code d'un script de dépôt | `src/plugins/script_preview.rs`, `src/plugins/highlight.rs`, `client/src/plugins/script_preview.rs` | `shared/src/highlight.rs` |
| Remuxage MKV | `src/media.rs`, `src/worker.rs` | `scripts/scenarios.py` |
| Page HTML et thème (CSS typé) | `frontend/src/lib.rs`, `frontend/src/theme/` | `frontend/README.md` |
| Interface (client Leptos WASM, ADR 0014) | `client/src/` (un dossier par écran), contrat dans `shared/src/` | `docs/adr/0014-client-leptos-wasm.md`, `Dockerfile` |
| Catalogue plugins (sections, propriétaire des blocs, dépendances) | `shared/src/plugins/vocab.rs`, `shared/src/plugins/subcategory.rs` (registre `subcategories.v1.json`, ADR 0023), `src/plugins/mod.rs`, `client/src/plugins/` | `docs/architecture/deux-depots.md`, `src/api/plugins.rs` |
| Paramètres (compte, thème, comptes, clés, système) | `client/src/settings/`, `client/src/theme.rs` | `src/api/system.rs`, `tests/system_status.rs` |
| Journal applicatif (onglet Activité > Journal) | `src/logs.rs`, `shared/src/logs.rs`, `client/src/jobs/logs.rs` | `src/main.rs` (écriture `tracing`), `docs/adr/0015-ia-et-mcp.md` ; tests : `cargo test -p forgearr-shared -p forgearr --lib logs` |
| Assistant IA (fournisseurs, connexion OAuth, outils MCP) | `src/ai/` (`provider.rs`, `oauth.rs`, `settings.rs`, `run.rs`, `history.rs`, `preview.rs`, `usage.rs`, `health.rs`, `diagnosis.rs`, `streaming.rs`), `shared/src/ai.rs`, `client/src/ai/` (tiroir, `run.rs`, `history.rs`), `client/src/settings/ai.rs`, `client/src/settings/ai_status.rs`, `client/src/jobs/diagnosis.rs` | `src/mcp/`, `docs/adr/0015-ia-et-mcp.md` ; tests : `cargo test -p forgearr-shared -p forgearr --lib ai` |
| Rôle de l'installation, enrôlement d'un worker | `src/instance.rs`, `src/worker_settings.rs`, `src/api/installation.rs` | `src/worker_http.rs`, `src/api/worker_view.rs`, `tests/installation_api.rs`, `tests/worker_web.rs`, `docs/adr/0011-installation-comptes-et-noeuds.md` |
| Secrets chiffrés, rotation de clé, mot de passe oublié | `src/secrets.rs`, `src/secret_rotation.rs`, `src/account_admin.rs` | `tests/secret_rotation.rs`, `tests/password_reset.rs`, `docs/adr/0011-installation-comptes-et-noeuds.md` |
| Configuration, TLS | `src/config.rs`, `tests/configuration.rs` | `compose.yaml`, `deploy/` ; déploiement dans le README |
| Laboratoire Docker | `package.json`, `scripts/dev.py`, `compose.yaml` | Scénarios du README, `deploy/fixtures.Dockerfile` |
| CI, branches, publication | `docs/adr/0002-flux-git.md` | Workflow concerné dans `.github/workflows/`, `scripts/ci/` |

Un seul crate et un seul binaire, deux modes : central ou worker. Les façades
`instance`, `central`, `execution`, `workers`, `plugins`, `flows`, `storage` et
`security` rendent les responsabilités visibles ; voir
[`architecture/deux-depots.md`](architecture/deux-depots.md) pour les frontières
entre socle et dépôt plugins. Seul le
central accède à SQLite (`/config/flux.db`) ; les workers échangent avec lui par HTTP
(TLS éventuel via le reverse proxy externe).
Les migrations sont embarquées et appliquées au démarrage du central.

## Vérifier selon le changement

- Documentation seule : vérifier liens, chemins et `git diff --check` ; pas de
  build Rust ni de démarrage Docker nécessaire.
- Rust : commandes de référence dans la rubrique « Vérifications Rust » du
  README ; toolchain épinglée dans `rust-toolchain.toml`, dépendances verrouillées.
  Les tests Rust actuels ne nécessitent ni serveur de base externe ni MKVToolNix installé.
- Chaîne média, baux, transferts ou Docker : tests d'intégration Rust de `tests/`
  (`cargo test`, exécutés par la CI). Ils ne prouvent ni la HA ni le
  fonctionnement sur plusieurs machines physiques.
- CI : vérifier le workflow concerné. Le job `docker-build` de `test.yml`
  construit l'image, la charge localement et attend `/health` sans rien publier ;
  l'image `:test` / `:test-<sha12>` n'est construite et poussée qu'à la fusion
  `dev → test` (job `docker-publish`, action `.github/actions/build-image`).
  Sur la PR `test → main`, `main-checks.yml` tire l'image `test-<sha12>` à promouvoir
  et démarre central et worker avec `compose.yaml`. Sans changement de code,
  les étapes de `test.yml` sont sautées et le check reste vert ; cela ne constitue
  pas une nouvelle exécution.

## Pièges à consulter pour le remuxage

- `mkvmerge` ne reconnaît pas `--` comme séparateur générique ; passer des chemins
  contrôlés. Garder `--output-charset UTF-8` pour son JSON sous locale minimale.
- Les identifiants de chapitres et les frontières de paquets peuvent changer :
  comparer le contenu et les flux encodés, pas le hash du conteneur remuxé.
- La publication du fichier précède la validation SQL : un crash peut laisser
  des fichiers orphelins. Ne pas ajouter de purge destructive implicite.
- Garder un seul fournisseur rustls : `tls-rustls-no-provider` pour axum-server,
  fournisseur ring via reqwest/SQLx.

## Recherches et contexte

- Commencer par `rg --files` ou `git ls-files`, puis `rg` sur les chemins utiles.
  Les exclusions Git écartent déjà `target/`, `var/`, `.tools/`, `.scenario/`,
  `node_modules/` et les répertoires locaux des agents.
- Éviter les parcours globaux avec `--no-ignore` ou `find .` : `target/` peut
  contenir des milliers de fichiers. Ne pas lire `Cargo.lock` en entier pour
  découvrir les dépendances directes : lire `Cargo.toml` ; cibler le verrou si utile.
- Dans AgentsRoom, lire l'index de mémoire puis uniquement les notes pertinentes.
  Les miroirs, hooks et configurations MCP sont gérés par l'application : ne pas
  les modifier. Leurs règles d'utilisation restent celles de la session.
- Les compétences partagées peuvent contenir des exemples d'autres projets :
  pour la stack et le flux Git, consulter les décisions locales dans les ADR.
