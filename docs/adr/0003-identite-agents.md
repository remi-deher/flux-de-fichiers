# 0003 — Identité cryptographique propre à chaque agent

Statut : accepté par l'utilisateur.

## Contexte et constat de sécurité

Dans l'implémentation initiale (v0.2.0), tous les agents partagent un secret
unique `WORKER_KEY` vérifié par le coordinateur (`src/auth.rs`), tandis que
l'identité de l'agent est purement déclarative :

- `POST /api/v1/workers` accepte n'importe quel `name` avec `WORKER_KEY` ;
- `POST /api/v1/claims` accepte n'importe quel `worker_id` dans le corps JSON
  (`ClaimRequest`) ;
- `POST /api/v1/jobs/{id}/heartbeat`, `POST /api/v1/jobs/{id}/failure`,
  `GET /api/v1/jobs/{id}/source` et `PUT /api/v1/jobs/{id}/result` reçoivent
  `worker_id` dans le corps ou l'en-tête `x-worker-id`.

Cette architecture présente trois risques de sécurité :

1. **Usurpation d'identité et détournement de tâches ciblées** : tout détenteur
   de `WORKER_KEY` peut appeler `POST /api/v1/claims` en fournissant le
   `worker_id` d'un autre agent et réclamer les tâches qui lui étaient
   spécifiquement destinées (`target_worker_id`), ou altérer son horodatage
   `last_seen`.
2. **Absence d'imputabilité** : les journaux d'audit et la colonne
   `jobs.worker_id` reflètent l'identifiant déclaré par le client HTTP et non
   une identité prouvée cryptographiquement.
3. **Impossibilité de révoquer un seul agent** : la compromission ou la mise
   hors service d'un hôte agent impose de remplacer `WORKER_KEY` simultanément
   sur le coordinateur et sur l'ensemble des agents actifs.

## Décision

**Option retenue : une clé secrète propre à chaque agent (option a), générée et
conservée par l'agent dans un volume `state` dédié, enregistrée au premier
démarrage via un jeton d'enrôlement à usage unique, et dont seule l'empreinte
SHA-256 est connue du coordinateur et stockée en base.**

Le coordinateur n'a plus de clé `WORKER_KEY` globale et ne voit jamais la clé
en clair d'un agent, y compris lors de son enrôlement. Chaque agent génère sa
propre clé Bearer de 256 bits à son premier démarrage, transmet uniquement son
empreinte SHA-256 accompagnée d'un jeton d'enrôlement à usage unique lié à son
nom, puis s'authentifie avec cette clé sur toutes ses requêtes.

## Alternatives écartées

### 1. mTLS avec certificat client par agent (option b)

| Critère | (a) Clé par agent, empreinte en base *(retenue)* | (b) mTLS avec certificat client par agent *(écartée)* |
|---|---|---|
| **Révocation d'un seul agent** | Immédiate en SQL (suppression de l'empreinte et marquage `revoked_at` sur la ligne de l'agent), sans redémarrage ni rechargement TLS. | Complexe : nécessite une liste de révocation (CRL) ou un vérificateur `rustls` personnalisé et un rechargement à chaud. |
| **Renouvellement / remplacement** | Révocation de l'agent, émission d'un nouveau jeton d'enrôlement à usage unique et réenrôlement avec un volume `state` neuf. | Émission d'un nouveau certificat X.509 signé par la CA interne, suivi des dates d'expiration et renouvellement périodique. |
| **Déploiement sur TrueNAS (middleware)** | Simple : un jeton d'enrôlement initial (`enrollment_token`), le certificat public `ca.crt` et un volume persistant `state` gérés via le middleware. | Lourd : distribution d'une paire `(client.crt, client.key)` propre à chaque NAS en plus de `ca.crt`, avec gestion stricte des droits UID `10001`. |
| **Complexité applicative** | Faible : conserve l'en-tête `Authorization: Bearer` et la séparation entre couche TLS de transport et authentification applicative. | Élevée : extraction de l'identité X.509 depuis `axum-server`/`rustls` vers les handlers Axum ; complique le passage par le proxy local `preview` (`nginx`). |
| **Compatibilité `deploy/worker.compose.yaml` et `scripts/dev.py`** | Ajout d'un volume `state:/state` et d'un secret d'amorçage `enrollment_token` ; aucune étape OpenSSL supplémentaire dans `scripts/dev.py`. | Impose d'ajouter `client_cert` et `client_key` dans `deploy/worker.compose.yaml` et de générer un CSR/certificat X.509 par agent dans `scripts/dev.py`. |

### 2. Variantes d'enrôlement écartées (E1, E2, E3)

- **E1 — Clé générée par le coordinateur sur `POST /api/v1/workers` + fichier
  d'empreintes de clés pour le laboratoire** : écartée car le coordinateur
  connaît la clé secrète de l'agent à sa création et l'opérateur doit copier
  manuellement une clé permanente vers l'hôte de l'agent.
- **E2 — Clé générée par le coordinateur par appel HTTP après démarrage dans
  `scripts/dev.py`** : écartée pour la même exposition initiale de la clé au
  coordinateur et parce qu'elle impose une orchestration en deux temps lors de
  `docker compose up`.
- **E3 — Sous-commande CLI administrateur écrivant directement en base** :
  écartée car elle impose un `docker exec` sur le coordinateur et contourne
  l'API HTTP utilisée par les scénarios et la future interface Leptos.

## Spécification de la solution retenue

### 1. Génération de la clé par l'agent et volume `state` dédié

- **Génération locale au premier démarrage** : si aucun fichier de clé n'est
  présent dans son répertoire d'état (`/state/worker_key`), l'agent génère
  lui-même 32 octets aléatoires via un générateur cryptographiquement sûr
  (`OsRng` / `getrandom`), encodés en 64 caractères hexadécimaux minuscules
  (256 bits d'entropie).
- **Volume persistant `state`** : la clé est écrite de manière atomique dans un
  volume dédié (`state:/state`, variable `STATE_DIR` valant `/state` par
  défaut), distinct du volume temporaire `work` (`/work`), avec des permissions
  `0600` (lisible uniquement par l'UID `10001`). Le conteneur conserve son
  système de fichiers racine en lecture seule (`read_only: true`).
- **Persistance sur le cycle de vie de l'agent** : la clé est conservée pour
  toute la durée de vie de l'agent ; elle est relue à chaque redémarrage du
  conteneur et lors des mises à jour d'image Docker.
- **Stockage de l'empreinte seule côté coordinateur** : le coordinateur ne
  stocke que `key_sha256` (`SHA-256(key.as_bytes())` en hexadécimal, 64
  caractères) dans la table `workers`. L'entropie de 256 bits rend toute
  attaque par dictionnaire ou précalcul infaisable tout en autorisant une
  recherche indexée en $O(1)$ à chaque requête.

### 2. Jeton d'enrôlement à usage unique et protocole d'enrôlement

- **Émission du jeton par l'administrateur** :
  - L'administrateur appelle un endpoint dédié sous `AdminKey`
    (`POST /api/v1/workers/enrollments`, corps `{"name": "artemis"}`).
  - Le coordinateur refuse la demande (`409 Conflict`) si un agent actif (non
    révoqué) porte déjà ce nom.
  - Il génère un jeton d'enrôlement aléatoire de 256 bits (64 caractères
    hexadécimaux), calcule son empreinte `token_sha256`, l'enregistre en base
    associée au `name` demandé et à une date d'expiration courte
    (`now() + ENROLLMENT_TTL_SECONDS`, durée définie par une constante nommée
    ou variable d'environnement, sans constante numérique magique), et retourne
    le jeton en clair **une seule fois** dans la réponse `201 Created`.
  - Conformément à `AGENTS.md`, seule l'empreinte `token_sha256` est stockée en
    base ; le jeton brut n'est jamais persisté ni journalisé.
- **Remise du jeton à l'agent** :
  - L'opérateur fournit ce jeton à l'agent par fichier secret
    (`ENROLLMENT_TOKEN_FILE`, par ex. `/run/secrets/enrollment_token`).
  - Ce fichier n'est lu par l'agent que lors de son premier démarrage, lorsqu'il
    doit s'enrôler. Une fois l'agent enrôlé et sa clé persistée dans `/state`,
    le jeton d'enrôlement est consommé et ignoré aux redémarrages suivants.
- **Enrôlement au premier démarrage (`POST /api/v1/workers`)** :
  - L'agent envoie `{"name": "<nom>", "key_sha256": "<sha256-hex>"}` avec le
    jeton d'enrôlement dans `Authorization: Bearer <enrollment_token>`.
  - Dans une transaction SQL atomique, le coordinateur :
    1. calcule `SHA-256(enrollment_token)` et verrouille le jeton correspondant ;
    2. rejette la requête (`401 Unauthorized` ou `409 Conflict`) si le jeton est
       inconnu, expiré (`expires_at <= now()`), déjà consommé (`used_at IS NOT
       NULL`), lié à un autre `name`, ou si un agent actif occupe déjà ce `name` ;
    3. marque le jeton comme consommé (`used_at = now()`) ;
    4. crée (ou réactive après révocation) la ligne `workers` avec `name`,
       `key_sha256` et `revoked_at = NULL`, puis retourne `Worker { id, name,
       last_seen }`.
  - La clé secrète de l'agent ne transite jamais sur le réseau lors de
    l'enrôlement : seule son empreinte `key_sha256` est envoyée.
- **Authentification courante après enrôlement** :
  - À chaque démarrage ultérieur (lorsque `/state/worker_key` existe déjà) et
    sur toutes les routes agent, l'agent s'authentifie par
    `Authorization: Bearer <clé>`.
  - L'identité (`worker.id`, `worker.name`) est déduite exclusivement de
    `SHA-256(clé)` en base.

### 3. Absence de rotation automatique et révocation unitaire

- **Pas de rotation automatique ni périodique** :
  - La clé d'un agent n'est renouvelée que lorsque l'agent sort du système ou
    est remplacé. Il n'existe pas d'endpoint `rotate-key`.
  - Le renouvellement d'un agent s'effectue par la séquence standard :
    révocation de l'agent existant (`POST /api/v1/workers/{id}/revoke`),
    émission d'un nouveau jeton d'enrôlement (`POST /api/v1/workers/enrollments`)
    et réenrôlement avec un volume `state` vierge.
- **Révocation unitaire (`POST /api/v1/workers/{id}/revoke` sous `AdminKey`)** :
  - La table `jobs` référence `workers(id)` via `worker_id` et
    `target_worker_id`. Afin de préserver l'intégrité référentielle et
    l'historique des tâches, la ligne `workers` n'est pas supprimée.
  - En une transaction SQL atomique :
    1. `UPDATE workers SET key_sha256 = NULL, revoked_at = now() WHERE id = $1` ;
    2. les jetons d'enrôlement non utilisés éventuels pour ce `name` sont
       invalidés ;
    3. les baux actifs de cet agent (`status = 'running' AND worker_id = $1`)
       sont immédiatement repris : remis en `'queued'` (si
       `attempts < MAX_ATTEMPTS` et non ciblés sur l'agent révoqué) ou passés en
       `'failed'` avec `error_code = 'worker_revoked'`, `lease_token = NULL`,
       `leased_until = NULL`.
  - Toute requête ultérieure de l'agent révoqué reçoit immédiatement
    `401 Unauthorized`. Les autres agents continuent de fonctionner sans
    interruption.

### 4. Cas limites et risques résiduels

- **Volume `state` perdu alors que le nom est déjà enrôlé** :
  - Si un conteneur agent perd son volume `state`, il génère une nouvelle clé
    au démarrage et tente de s'enrôler. Comme le nom est déjà associé à un agent
    actif en base (et que le jeton initial est déjà consommé ou expiré), le
    coordinateur refuse l'enrôlement.
  - Ce blocage est volontaire : il empêche un attaquant ou un conteneur mal
    configuré d'écraser l'identité d'un agent actif. L'agent journalise une
    erreur explicite (`worker_enrollment_failed`) et s'arrête proprement ;
    l'administrateur doit révoquer l'agent en base puis émettre un nouveau jeton
    d'enrôlement.
- **Copie du volume `state` sur une autre machine (risque résiduel de clonage)** :
  - Si le contenu du volume `state` (`/state/worker_key`) est copié sur un
    second hôte, les deux instances présenteront la même clé Bearer et seront
    indiscernables pour le coordinateur (limite inhérente à tout secret Bearer).
  - *Mesure opérationnelle et remède* : un volume `state` est strictement local
    à son hôte et ne doit jamais être dupliqué ni restauré sur une autre machine
    en parallèle. En cas de suspicion de copie ou de compromission, la
    révocation unitaire (`POST /api/v1/workers/{id}/revoke`) coupe
    immédiatement l'accès des deux instances.
- **Réinitialisation partielle du laboratoire `npm run dev` (volumes `state`
  supprimés mais base PostgreSQL conservée)** :
  - En développement local, `scripts/dev.py` fournit au coordinateur un fichier
    d'amorçage `WORKER_ENROLLMENT_SEEDS_FILE` contenant les couples
    `<nom>:<token_sha256>` des trois agents locaux.
  - Si un développeur supprime les volumes `worker_{1,2,3}_state` sans supprimer
    le volume `postgres`, un agent actif existe déjà en base pour `worker-1..3`.
  - Pour éviter un blocage silencieux en laboratoire tout en gardant la règle
    stricte en production : la procédure nominale de remise à zéro complète du
    laboratoire est `docker compose down -v` (qui purge simultanément `postgres`
    et `worker_{1,2,3}_state`). De plus, lorsque `WORKER_ENROLLMENT_SEEDS_FILE`
    est configuré (mode laboratoire uniquement), si l'agent présente un jeton
    figurant dans ce fichier d'amorçage, le coordinateur autorise le
    ré-enrôlement du nom correspondant en remplaçant son `key_sha256`. En
    production (`WORKER_ENROLLMENT_SEEDS_FILE` absent), un jeton est
    strictement à usage unique et un nom actif ne peut jamais être ré-enrôlé
    sans révocation préalable.
- **Protection des secrets** :
  - Ni le jeton d'enrôlement ni la clé privée de l'agent ne sont jamais
    affichés dans les journaux applicatifs ou passés en argument de ligne de
    commande (`ps` / historique shell).

## Impacts techniques

### 1. Migration SQL (`migrations/0003_worker_identity.sql`)

- Modification de la table `workers` :
  - ajout de `key_sha256 TEXT UNIQUE CHECK (key_sha256 ~ '^[0-9a-f]{64}$')` ;
  - ajout de `revoked_at TIMESTAMPTZ` ;
  - contrainte `CHECK ((revoked_at IS NULL) = (key_sha256 IS NOT NULL))`
    garantissant qu'un agent actif possède toujours une empreinte de clé et
    qu'un agent révoqué n'en possède plus ;
  - les lignes `workers` préexistantes éventuelles sont marquées
    `revoked_at = now(), key_sha256 = NULL` lors de la migration jusqu'à leur
    enrôlement.
- Création de la table `worker_enrollments` :
  - `token_sha256 TEXT PRIMARY KEY CHECK (token_sha256 ~ '^[0-9a-f]{64}$')` ;
  - `worker_name TEXT NOT NULL CHECK (length(worker_name) BETWEEN 1 AND 80)` ;
  - `expires_at TIMESTAMPTZ NOT NULL` ;
  - `used_at TIMESTAMPTZ` ;
  - `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`.

### 2. Configuration et API (`src/config.rs`, `src/auth.rs`, `src/api*.rs`, `src/worker.rs`, `docs/openapi.yaml`)

- **Coordinateur (`Config`)** :
  - suppression de `WORKER_KEY` / `WORKER_KEY_FILE` ;
  - ajout optionnel de `WORKER_ENROLLMENT_SEEDS_FILE` (amorçage des empreintes
    de jetons d'enrôlement pour le laboratoire) et de la durée de validité des
    jetons d'enrôlement (`ENROLLMENT_TTL_SECONDS`, constante nommée par défaut).
- **Agent (`WorkerConfig`)** :
  - suppression de `WORKER_KEY` / `WORKER_KEY_FILE` obligatoire au démarrage ;
  - ajout de `STATE_DIR` (défaut `/state`, contenant `worker_key`) et de
    `ENROLLMENT_TOKEN_FILE` (lu uniquement si `/state/worker_key` est absent).
- **Authentification (`src/auth.rs`)** :
  - `authorize_admin(&headers, &config.admin_key)` vérifie `ADMIN_KEY` en temps
    constant ;
  - `authorize_worker(&headers, &pool) -> AppResult<Worker>` calcule le SHA-256
    hexadécimal du jeton Bearer et charge l'agent actif (`SELECT * FROM workers
    WHERE key_sha256 = $1 AND revoked_at IS NULL`). Retourne `401 Unauthorized`
    si aucune ligne active ne correspond.
- **Routes modifiées ou ajoutées** :
  - `POST /api/v1/workers/enrollments` (sous `AdminKey`) : émet un jeton
    d'enrôlement à usage unique lié à `name`, stocke `token_sha256` et retourne
    le jeton en clair une seule fois (`201 Created`).
  - `POST /api/v1/workers` (sous `Bearer <enrollment_token>`) : reçoit
    `{name, key_sha256}`, vérifie et consomme le jeton d'enrôlement, enregistre
    `key_sha256` sur l'agent et retourne `Worker { id, name, last_seen }`.
  - `GET /api/v1/workers/me` (sous `WorkerKey`) : permet à un agent déjà doté de
    `/state/worker_key` de récupérer son `Worker { id, name, last_seen }` au
    redémarrage sans consommer de jeton d'enrôlement.
  - `POST /api/v1/workers/{id}/revoke` (sous `AdminKey`) : révoque l'agent et
    libère ses baux actifs (`204 No Content`).
  - `POST /api/v1/claims`, `POST /api/v1/jobs/{id}/heartbeat`,
    `POST /api/v1/jobs/{id}/failure`, `GET /api/v1/jobs/{id}/source`,
    `PUT /api/v1/jobs/{id}/result` : l'identité de l'agent est exclusivement
    `worker.id` issu de `authorize_worker`. Le champ `worker_id` du corps
    (`ClaimRequest`, `LeaseRequest`, `Failure`) et l'en-tête `x-worker-id` sont
    supprimés (ou rejetés en `401` s'ils divergent).

### 3. Déploiement (`deploy/worker.compose.yaml`)

- Ajout du volume persistant `state:/state` monté dans le conteneur agent
  (`STATE_DIR: /state`) en complément de `work:/work` et `logs:/logs`.
- Remplacement du secret `worker_key` (`WORKER_KEY_FILE`) par
  `enrollment_token` (`ENROLLMENT_TOKEN_FILE: /run/secrets/enrollment_token`,
  alimenté par `${ENROLLMENT_TOKEN_PATH}`), utilisé au premier démarrage pour
  enrôler la clé générée dans `/state/worker_key`.

### 4. Laboratoire de développement (`scripts/dev.py` et `compose.yaml`)

- `scripts/dev.py` génère trois jetons d'enrôlement locaux
  (`var/secrets/worker_1_enrollment_token`, `worker_2_enrollment_token`,
  `worker_3_enrollment_token`) ainsi que `var/secrets/worker_enrollment_seeds`
  contenant uniquement les trois lignes `worker-1:<token_sha256>`,
  `worker-2:<token_sha256>`, `worker-3:<token_sha256>`.
- `compose.yaml` déclare trois services `worker-1`, `worker-2`, `worker-3` (via
  une ancre commune) dotés chacun de leur volume persistant
  `worker_{1,2,3}_state:/state` et de leur secret
  `worker_{1,2,3}_enrollment_token`. Le coordinateur monte
  `worker_enrollment_seeds` (`WORKER_ENROLLMENT_SEEDS_FILE`).

### 5. Scénarios et tests (`scripts/scenarios.py`, `tests/`)

- Les agents synthétiques de test (`ghost`, `reserved`) sont créés en demandant
  un jeton d'enrôlement via `POST /api/v1/workers/enrollments` (avec `ADMIN`),
  en générant une clé locale de test et en appelant `POST /api/v1/workers` avec
  `SHA-256(clé)`.
- Nouveaux scénarios de sécurité exigés pour le ticket d'implémentation (#5) :
  - **Jeton d'enrôlement réutilisé** : un second appel à `POST /api/v1/workers`
    avec un jeton déjà consommé est refusé (`401`) ;
  - **Jeton d'enrôlement expiré** : un jeton dont `expires_at` est dépassé est
    refusé (`401`) ;
  - **Jeton d'enrôlement d'un autre nom** : utiliser un jeton émis pour `agent-a`
    en déclarant `name: "agent-b"` est refusé (`401`/`403`) ;
  - **Nom déjà enrôlé** : tenter d'émettre un jeton ou d'enrôler un nom déjà
    détenu par un agent actif non révoqué est refusé (`409`) ;
  - **Usurpation sur tâche ciblée** : une tâche ciblée sur l'agent A ne peut pas
    être réclamée avec la clé de l'agent B ;
  - **Isolation des baux** : l'agent B ne peut ni renouveler, ni lire la source,
    ni publier le résultat ou l'échec d'un bail appartenant à l'agent A ;
  - **Révocation unitaire** : `POST /api/v1/workers/{id}/revoke` invalide
    immédiatement la clé de l'agent ciblé (`401`) et remet son bail en file sans
    affecter les autres agents.
