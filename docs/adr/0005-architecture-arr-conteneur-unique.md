# 0005 — Architecture simplifiée style *arr : conteneur unique, port HTTP unique, SQLite /config et auto-amorçage

Statut : accepté par l'utilisateur le 10 novembre 2026.

## Contexte et limites du modèle multi-conteneurs initial

Dans la conception initiale (ADR 0001, ADR 0003), l'architecture imposait une séparation stricte des rôles en trois conteneurs distincts :
1. **Un conteneur PostgreSQL** (port 5432) pour les métadonnées et la coordination des baux.
2. **Un conteneur coordinateur Rust Axum** exposé obligatoirement en HTTPS direct sur le port 3443 (avec certificats TLS et autorité de certification `CA_CERT` obligatoires pour les écoutes non-loopback).
3. **Un conteneur reverse proxy Nginx** (port 3000) pour relayer les requêtes web et servir les fichiers statiques de l'interface Leptos.

Cette organisation présentait plusieurs inconvénients majeurs en pratique :
- **Complexité de déploiement** : obligation d'utiliser un fichier `docker-compose.yaml` lourd, rendant le déploiement sur les NAS (TrueNAS, Synology) et environnements Docker standalone fastidieux.
- **Gestion lourde des certificats et secrets** : génération préalable obligatoire d'une autorité de certification (CA), d'une paire clé/certificat TLS et injection manuelle des variables secrètes `ADMIN_KEY` et `WORKER_KEY` avant le moindre démarrage.
- **Surconsommation de ressources** : maintien d'une instance PostgreSQL complète et d'un processus Nginx pour une charge utilisateur mono-foyer ou petite équipe.

## Décision utilisateur du 10 novembre 2026

L'utilisateur a validé le passage au **modèle simplifié style *arr (Sonarr, Radarr)**, caractérisé par un déploiement ultra-simple en conteneur unique autonome.

### 1. Conteneur unique et port HTTP unique (3000)

- L'application `flux-de-fichiers` s'exécute dans un **conteneur Docker unique**.
- Le coordinateur écoute directement en **HTTP clair** sur un port unique via l'adresse de liaison `BIND_ADDR` (ou `BIND`, par défaut `0.0.0.0:3000`).
- La contrainte imposant le TLS obligatoire hors boucle locale (`127.0.0.1`) et le conteneur Nginx séparé sont supprimés.
- Si une sécurisation TLS externe est requise par l'infrastructure de l'utilisateur, elle est déléguée au reverse proxy d'entrée de son choix (Traefik, Caddy, Nginx Proxy Manager).

### 2. Base de données embarquée SQLite dans `/config`

- PostgreSQL est remplacé par **SQLite** via `sqlx` (feature `sqlite` avec runtime Tokio).
- La base de données réside par défaut dans `${CONFIG_DIR}/flux.db` (défaut `/config/flux.db`).
- Le moteur SQLite est configuré conformément aux conventions backend :
  - Mode journal : `WAL` (`journal_mode = WAL`) permettant des lectures concurrentes et des écritures optimisées ;
  - Synchronisation : `NORMAL` (`synchronous = NORMAL`) ;
  - Clés étrangères actives : `foreign_keys = ON` ;
  - Temporisation sur verrou : `busy_timeout = 5s` ;
  - Attribution atomique des baux et publication des résultats réalisées dans une transaction immédiate (`pool.begin_with("BEGIN IMMEDIATE")`), garantissant l'exclusivité d'attribution et prévenant tout échec de promotion de verrou `SQLITE_BUSY` sous accès concurrent.

### 3. Auto-amorçage complet du répertoire `/config` (`CONFIG_DIR`)

Au premier démarrage du coordinateur :
1. Le répertoire `${CONFIG_DIR}` (défaut `/config`, avec repli automatique sur `var/config` en développement local hors conteneur) est créé s'il n'existe pas.
2. Le fichier de base de données `${CONFIG_DIR}/flux.db` est automatiquement initialisé (`create_if_missing(true)`) et les migrations SQLx y sont appliquées.
3. Si les clés secrètes `ADMIN_KEY` (ou `ADMIN_KEY_FILE`) et `WORKER_KEY` (ou `WORKER_KEY_FILE`) ne sont pas fournies par l'environnement :
   - Le coordinateur génère automatiquement deux clés cryptographiques distinctes de 256 bits aléatoires (32 octets encodés en hexadécimal sur 64 caractères) ;
   - Ces clés sont enregistrées dans `${CONFIG_DIR}/admin_key` et `${CONFIG_DIR}/worker_key` avec des permissions strictes `0600` (lecture/écriture réservée au propriétaire) ;
   - Lors des démarrages ultérieurs, ces fichiers sont relus automatiquement.
4. Un endpoint d'administration authentifié `GET /api/v1/config` est exposé, permettant à l'interface utilisateur de récupérer la configuration du coordinateur (notamment la clé worker pour connecter un agent de traitement distant en un clic).

### 4. Serveur HTTP unifié Axum (UI Leptos + API)

Le serveur Axum dessert désormais l'ensemble des fonctionnalités sur le port 3000 :
- `/health` : sonde de santé pour l'orchestrateur Docker ;
- `/api/v1/*` : endpoints d'API REST (jobs, fichiers, workers, sessions, configuration, transferts) ;
- Fichiers statiques du frontend Leptos : servis depuis `STATIC_DIR` (défaut `/usr/share/flux-de-fichiers/frontend`, avec repli sur `frontend/dist` en développement) avec repli SPA automatique vers `index.html` pour les routes de navigation d'interface (hors requêtes sous `/api/`).

### 5. Communication des workers distants en HTTP

- Les workers distants (`src/worker.rs`) peuvent désormais communiquer avec le coordinateur en `http://` (ou `https://` si celui-ci est exposé derrière un nom de domaine avec certificat public).
- Le certificat d'autorité interne (`CA_CERT`) n'est plus obligatoire.

## Conséquences

- **Simplicité opérationnelle maximale** : une simple commande `docker run -p 3000:3000 -v /chemin/config:/config -v /chemin/data:/data flux-de-fichiers` suffit à faire tourner le service complet.
- **Consommation réduite** : disparition des processus Nginx et PostgreSQL, démarrage instantané.
- **Rétrocompatibilité API** : les contrats d'API `/api/v1/*` restent inchangés pour le frontend Leptos et les agents de traitement.
