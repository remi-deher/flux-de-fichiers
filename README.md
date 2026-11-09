# Flux de Fichiers

Chaîne de remuxage Matroska sans réencodage, exécutée sous Docker. Un coordinateur
Rust/Axum et PostgreSQL distribuent les tâches à un ou plusieurs agents Rust.
Les agents utilisent **MKVToolNix (`mkvmerge`)**. L'interface de pilotage Leptos est disponible sur http://localhost:3000.
FFmpeg est uniquement un outil de fabrication des médias synthétiques des tests.

Pour contribuer : [guide de développement ciblé](docs/developpement.md)
(points d'entrée, choix des vérifications et pièges).

## Démarrage local

Prérequis : Docker Engine avec Compose v2, Node/npm, Python 3 et OpenSSL.

```sh
npm run dev
```

Cette commande prépare une autorité de certification et des secrets **locaux de
développement**, construit l'image, puis lance PostgreSQL, un coordinateur et trois
agents dans des conteneurs distincts. Le terminal reste attaché aux journaux.
Le premier build Rust peut prendre plusieurs minutes.

- `http://localhost:3000` : Interface de pilotage Leptos et proxy local vers l'API.
- `https://localhost:3443` : API TLS directe, avec `var/secrets/ca.crt` comme CA.
- Les deux ports sont publiés uniquement sur la boucle locale. PostgreSQL n'est
  pas publié sur l'hôte. Les communications internes passent par TLS, y compris SQLx/PostgreSQL.
- Les journaux JSON du coordinateur sont dans son volume `/logs`, ceux des agents
  dans leurs volumes `/logs`. Les accès incluent un identifiant `x-request-id`.

Déposer des `.mkv` dans `var/input/`. Le montage est en lecture seule dans le
coordinateur ; les sources ne sont jamais remplacées. Les résultats vont dans le
volume Docker `data`, et sont récupérables via l'API.

## Scénarios reproductibles

Dans un second terminal, une fois les conteneurs en bonne santé :

```sh
npm run scenarios
```

Le scénario utilise des fichiers préfixés `scenario-…` dans `var/input`, sans
consommer de médias utilisateur. Il vérifie :

- Un agent actif (deux autres temporairement suspendus), remux réel et conservation
  du contenu encodé vidéo/audio/sous-titres, des chapitres, tags et pièces jointes.
- Trois agents, affectation explicite et concurrence sans double attribution.
- Disparition d'un agent, expiration de bail, reprise et refus de l'ancien bail.
- Sources invalides ou modifiées, résultats invalides et épuisement des reprises.
- Authentification, rôles et chemins interdits ; persistance après redémarrage SQL.

Le test accélère l'expiration **de ses propres tâches seulement**, en SQL ; il ne
modifie pas la durée des baux de l'application. Rapport : `var/scenario-report.json`.
Les trois agents sont sur le même hôte Docker : cela ne valide pas une panne de
machine physique ni la haute disponibilité.

## API et authentification

Contrat : [docs/openapi.yaml](docs/openapi.yaml). Les clés sont générées dans
`var/secrets/admin_key` et `worker_key` (répertoire ignoré et réservé à l'utilisateur).
Ne jamais afficher ces clés dans les journaux ou les placer dans une commande partagée.
Un client utilise `Authorization: Bearer …` ; la clé administrateur soumet et lit les
tâches, la clé agent enregistre les agents et exécute les baux. Une clé d'agent
identifie ici un groupe de confiance ; l'isolation cryptographique par agent n'est
pas encore implémentée.

Créer une tâche : `POST /api/v1/jobs`, corps
`{"source_name":"exemple.mkv","target_worker_id":null}`. Un identifiant d'agent
peut remplacer `null`. Lire son état avec `GET /api/v1/jobs/{id}` et télécharger
le résultat avec `GET /api/v1/jobs/{id}/output` lorsqu'il est `succeeded`.

Les tâches ont un bail de 120 s renouvelé toutes les 20 s. Les expirations sont
réévaluées à chaque demande de travail ; trois tentatives maximum. Une tâche liée
à un agent précis attend cet agent et n'est pas transférée automatiquement à un
autre. Les erreurs média sont terminales. Une panne de réseau ou de base laisse
le bail expirer lorsque le signalement est impossible ; la reconnexion est automatique.

Les téléchargements et téléversements sont diffusés en flux, limités à 512 Mio
par défaut (`MAX_FILE_BYTES`), avec délais bornés. Un remux a au maximum une heure.
Les codes MKVToolNix 0 et 1 sont acceptés ; 1 est conservé dans `warning`, 2 échoue.
Les empreintes source/résultat et tailles sont enregistrées. La comparaison des
pistes et métadonnées est structurelle ; la comparaison intégrale des flux encodés
est effectuée dans les scénarios de test.

## Déploiement sur plusieurs machines

La même image exécute `coordinator` ou `worker`. En mode `coordinator`, un agent
intégré est activé par défaut sous le nom `coordinator-<HOSTNAME>` : il prend les
baux via PostgreSQL et traite les tâches sans conteneur worker séparé. Il lit la
source directement dans `INPUT_DIR` monté en lecture seule. MKVToolNix, la
validation des métadonnées, le contrôle du bail et la publication immuable sont
partagés avec le chemin des agents distants.

`LOCAL_WORKER=false` (ou `0`) désactive l'agent intégré pour un coordinateur
dédié à la coordination. Une tâche ciblée sur un worker reste réservée à celui-ci.
Le traitement local utilise les ressources CPU et disque du coordinateur et ne
déclenche aucune bascule automatique entre coordinateurs. Le Compose de laboratoire
désactive ce mode afin de garder les scénarios de disponibilité des workers
déterministes.
[deploy/worker.compose.yaml](deploy/worker.compose.yaml) permet de lancer uniquement
un agent sur un autre hôte, avec une image immuable et des secrets fournis par fichiers. Le fichier Compose fourni est
une **configuration locale de développement**, pas un déploiement automatique.

Sur l'hôte central : PostgreSQL, coordinateur, volume d'entrées en lecture seule,
volumes persistants de résultats et de journaux. Sur chaque machine de traitement :
image applicative en mode `worker`, volume de travail et journaux locaux, sans
connexion directe à PostgreSQL ni montage des fichiers source.

Paramètres coordinateur : `BIND`, `DATABASE_URL_FILE`, `ADMIN_KEY_FILE`,
`WORKER_KEY_FILE`, `TLS_CERT`, `TLS_KEY`, `INPUT_DIR`, `DATA_DIR`, `LOG_DIR`,
`LOCAL_WORKER` (`true` par défaut), `LOCAL_WORKER_NAME` (sinon `coordinator-<HOSTNAME>`).
Paramètres agent : `COORDINATOR_URL=https://nom-central:3443`, `CA_CERT`,
`WORKER_KEY_FILE`, `WORKER_NAME` (unique ; sinon nom du conteneur), `WORK_DIR`, `LOG_DIR`.
Le certificat serveur doit couvrir le nom DNS utilisé par les agents. En dehors
de la boucle locale, TLS est obligatoire et la vérification des certificats reste
activée. Fournir les certificats et secrets via l'orchestrateur, et publier le
port TLS seulement sur le réseau voulu. Ne pas copier les secrets de développement
pour un déploiement réel. Sur TrueNAS, les apps passent par le middleware.

Un crash entre publication du fichier et validation SQL peut laisser un fichier
orphelin inaccessible par l'API ; de même, un arrêt brutal peut laisser du staging
ou un répertoire de travail. Aucune purge automatique destructive n'est effectuée.
Les données, journaux et fichiers orphelins sont à inclure dans la politique de
rétention ; les médias peuvent saturer un volume. Les fichiers de résultats sont
immuables et une tâche n'est annoncée comme réussie qu'après validation SQL.

Pour arrêter le laboratoire, utiliser le terminal Docker Compose (`Ctrl-C`).
Ne pas supprimer les volumes si les résultats doivent être conservés.

## Vérifications Rust

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

`Cargo.lock` est versionné. L'image s'exécute sans privilèges avec un utilisateur
non root. Les essais distribués sont décrits ci-dessus ; aucune HA n'est activée
en fonction du nombre d'agents.
