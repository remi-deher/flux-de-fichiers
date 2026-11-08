# Développement : lire selon la tâche

`AGENTS.md` contient les règles permanentes ; `CLAUDE.md` l'importe sans copie.
Ce guide se consulte à la demande. Le README reste la référence d'utilisation
(démarrage, API, scénarios, déploiement), les ADR celle des décisions.

## Points d'entrée

| Travail | Lire d'abord | Compléter si nécessaire |
|---|---|---|
| API, authentification | `src/api.rs`, `src/auth.rs`, `src/api/jobs.rs` | Route concernée dans `docs/openapi.yaml`, `tests/api_security.rs` |
| Baux, attribution, reprises | `src/repository.rs`, `src/domain.rs` | `migrations/`, `src/worker.rs` |
| Transferts, intégrité, publication | `src/api/transfers.rs`, `src/storage.rs` | `src/media.rs`, `src/worker.rs` |
| Remuxage MKV | `src/media.rs`, `src/worker.rs` | `scripts/scenarios.py` |
| Configuration, TLS | `src/config.rs`, `tests/configuration.rs` | `compose.yaml`, `deploy/` ; déploiement dans le README |
| Laboratoire Docker | `package.json`, `scripts/dev.py`, `compose.yaml` | Scénarios du README, `deploy/fixtures.Dockerfile` |
| CI, branches, publication | `docs/adr/0002-flux-git.md` | Workflow concerné dans `.github/workflows/`, `scripts/ci/` |

Un seul crate et un seul binaire, deux modes : coordinateur ou agent. Seul le
coordinateur accède à PostgreSQL ; les agents échangent avec lui par HTTPS.
Les migrations sont embarquées et appliquées au démarrage du coordinateur.

## Vérifier selon le changement

- Documentation seule : vérifier liens, chemins et `git diff --check` ; pas de
  build Rust ni de démarrage Docker nécessaire.
- Rust : commandes de référence dans la rubrique « Vérifications Rust » du
  README ; toolchain épinglée dans `rust-toolchain.toml`, dépendances verrouillées.
  Les tests Rust actuels ne nécessitent ni PostgreSQL actif ni MKVToolNix installé.
- Chaîne média, baux, transferts ou Docker : `npm run dev`, puis
  `npm run scenarios` sur le laboratoire local disponible. Les scénarios suspendent
  des agents et redémarrent PostgreSQL : utiliser un laboratoire de développement.
  Ils ne prouvent ni la HA ni le fonctionnement sur plusieurs machines physiques.
- CI : vérifier le workflow concerné. Les scénarios Docker ne sont pas exécutés
  par `test.yml` ; une CI verte ne suffit pas à les déclarer validés.

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
