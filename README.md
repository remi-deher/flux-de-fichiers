# Flux de Fichiers

Chaîne de traitement de fichiers média (encodage par plugins), dans un conteneur unique « à la
*arr ». Un central Rust (Axum/Tokio) stocke son état dans SQLite (`/config/flux.db`),
sert l'interface de pilotage Leptos et distribue les tâches à un worker intégré ou à des
workers distants. Le remuxage Matroska utilise **MKVToolNix (`mkvmerge`)** ; la sonde est
**MediaInfo**. L'encodage (FFmpeg, encodeurs matériels) arrive par les plugins, jamais
embarqué dans l'image ([ADR 0012](docs/adr/0012-perimetre-perte-de-bail-et-versions-de-flows.md)).

Pour contribuer : [guide de développement ciblé](docs/developpement.md)
(points d'entrée, choix des vérifications et pièges). Les décisions d'architecture sont dans
[docs/adr](docs/adr), le flux Git dans [docs/adr/0002-flux-git.md](docs/adr/0002-flux-git.md).

## Démarrage

L'image est construite et testée par la CI GitHub (`dev → test → main`) puis publiée sur
Docker Hub : `mrcryllix/flux-de-fichiers`. Les tags `test-<sha>` sont les images validées,
`X.Y.Z`, `X.Y` et `latest` sont promus à chaque release, sans recompilation.

```sh
docker run -d --name flux -p 3000:3000 \
  -v flux-config:/config -v flux-data:/data -v /chemin/des/mkv:/input:ro \
  mrcryllix/flux-de-fichiers:latest central
```

L'interface est servie sur http://localhost:3000 (un seul port HTTP ; TLS optionnel
via un reverse proxy externe). Au premier démarrage, le central crée dans `/config` :

| Fichier | Rôle |
|---------|------|
| `flux.db` | base SQLite (tâches, flux, bibliothèques, réglages) |
| `secret_key` | clé de chiffrement des secrets, générée à l'installation (0600) |
| `plugins/<dépôt>/` | copie locale de chaque dépôt de plugins synchronisé |

**Première ouverture** : la page d'installation demande le nom du central, puis un
identifiant et un mot de passe administrateur ; la session s'ouvre ensuite toute seule
([ADR 0011](docs/adr/0011-installation-comptes-et-noeuds.md)). Tant que l'installation n'est
pas faite, la page est ouverte à qui atteint le port : à faire sur un réseau de confiance.
Le secret n'est jamais affiché ; pour le fournir par fichier, `SECRET_KEY_FILE`.

Un worker distant utilise la même image : `docker run … worker` avec `CENTRAL_URL`,
`WORKER_KEY_FILE` (clé d'API de rôle worker créée dans Réglages → Clés d'API),
`WORKER_NAME`, `WORK_DIR` (voir
[deploy/worker.compose.yaml](deploy/worker.compose.yaml)).

## Configuration

`BIND` (`0.0.0.0:3000`), `CONFIG_DIR` (`/config`), `DATA_DIR`, `INPUT_DIR`, `LOG_DIR`,
`MAX_FILE_BYTES` (512 Mio), `LOCAL_WORKER` (`true` : worker intégré), `WEB_ORIGIN`,
`WEB_SESSION_TTL_SECONDS`, `TLS_CERT` et `TLS_KEY` (ensemble), `TOOLS_CACHE_DIR`.
Mot de passe oublié par courriel (ADR 0015) : `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS` (`starttls`, `tls`
ou `none`), `SMTP_USERNAME`, `SMTP_FROM` et `SMTP_PASSWORD` (ou `SMTP_PASSWORD_FILE`), avec `WEB_ORIGIN`
pour bâtir le lien. Sans eux, seule la ligne de commande `reset-password` reste.
Plugins : `PLUGINS_DIR` (dossier de catalogue local de développement, prioritaire) et
`PLUGIN_ALLOW_PRIVATE_HOSTS=1` pour autoriser un dépôt sur un réseau privé.

## Plugins, outils et secrets

Le catalogue ne vient que des dépôts de plugins (aucun catalogue embarqué). Le dépôt
officiel, `flux-de-fichiers-plugins`, est synchronisé au démarrage, toutes les 6 h et à la
demande dans `/config/plugins/<id>`. Chaque worker installe lui-même ses outils depuis leur
contrat de distribution (téléchargement https, SHA-256 vérifié avant exécution) ; une sonde
installée est exigée avant toute tâche sur fichier. Les clés d'API et les en-têtes des
webhooks sont chiffrés au repos et ne sont jamais renvoyés par l'API. Détails :
[ADR 0010](docs/adr/0010-depots-de-plugins-synchronises.md).

## API et authentification

Contrat : [docs/openapi.yaml](docs/openapi.yaml). Une machine utilise
`Authorization: Bearer fdf_…` : une clé d'API de rôle admin crée et lit les tâches et
administre la plateforme, une clé de rôle worker enregistre les workers et exécute les baux
(clés nommées, révocables, affichées une seule fois). Le navigateur utilise un compte et une
session cookie avec CSRF ([docs/api/session-web.md](docs/api/session-web.md)).

Créer une tâche : `POST /api/v1/jobs`, corps `{"source_name":"exemple.mkv","target_worker_id":null}`.
Lire son état avec `GET /api/v1/jobs/{id}` et télécharger le résultat avec
`GET /api/v1/jobs/{id}/output` lorsqu'il est `succeeded`.

Les tâches ont un bail de 120 s renouvelé toutes les 20 s. Un bail expiré fait échouer la
tâche (`lease_expired`) sans la remettre en file ; un administrateur peut la relancer avec
`POST /api/v1/jobs/{id}/retry`, trois tentatives au maximum.
Les erreurs média sont terminales. Les transferts sont diffusés en flux, limités par
`MAX_FILE_BYTES`. Un remux dure au plus une heure ; les codes MKVToolNix 0 et 1 sont
acceptés (1 est conservé en avertissement), 2 échoue. Les empreintes source et résultat
sont enregistrées.

Un crash entre la publication du fichier et la validation SQL peut laisser un fichier
orphelin ; aucune purge destructive n'est automatique. Les résultats sont immuables et
une tâche n'est annoncée réussie qu'après validation SQL.

## Vérifications

Les builds Docker et les suites de tests s'exécutent sur la CI GitHub, pas en local. Les
vérifications de référence (celles de la CI) :

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

`Cargo.lock` est versionné ; l'image s'exécute sans privilèges avec un utilisateur non
root. Aucune haute disponibilité n'est activée en fonction du nombre de workers.
