# 0011 — Installation guidée, comptes, clés d'API et nœuds worker à interface

Statut : conception validée par l'utilisateur (13 nov. 2026) ; implémentation en quatre phases.

## Contexte

L'accès repose aujourd'hui sur deux secrets partagés (`ADMIN_KEY`, `WORKER_KEY`) générés en clair dans
`/config`, le rôle du processus est fixé par l'argument de lancement, et un worker n'a ni serveur HTTP
ni interface. L'objectif est un démarrage sans manipulation : un assistant au premier lancement,
des comptes avec mot de passe pour les humains, des clés d'API pour les machines.

## Décisions

1. **Démarrage sans commande** : un conteneur non configuré sert une page d'installation sur le port
   3000. Le rôle (centrale ou worker) est conservé dans `/config` ; `coordinator` et `worker` restent
   acceptés comme arguments pour l'automatisation. Le test de santé devient HTTP pour les deux rôles.
2. **Installation de la centrale** : nom de la centrale, identifiant et mot de passe du compte
   administrateur, puis session ouverte. Le secret de chiffrement est généré à ce moment, sans
   interaction. La page est ouverte tant que l'installation n'est pas terminée (fenêtre de
   réclamation acceptée sur réseau de confiance, un bandeau le signale).
3. **Comptes** : mot de passe haché en Argon2id (salage par compte), connexions limitées en nombre
   d'essais, sessions par cookie avec CSRF (existant). Un seul rôle au départ, administrateur ;
   le modèle de droits est en place pour en ajouter.
4. **Clés d'API** pour les machines : une clé par nœud, nommée et révocable, seul son haché SHA-256
   est stocké, affichée une seule fois. Plus d'`ADMIN_KEY` ni de `WORKER_KEY` : l'installation de test
   est remise à zéro, aucune migration n'est écrite.
5. **Enrôlement d'un worker** : l'utilisateur saisit l'adresse de la centrale, l'identifiant et le mot
   de passe d'un administrateur, et le nom du nœud. La centrale crée le nœud et sa clé, renvoyée une
   fois ; le mot de passe n'est jamais conservé par le worker.
6. **Deux axes d'accès** :
   - le **type de nœud** (centrale, worker) décide de ce qui existe sur le serveur ;
   - le **rôle** de l'utilisateur décide de ce qu'il voit et fait.
   Chaque panneau et chaque route déclarent le droit exigé et les types de nœud qui l'hébergent.
   `GET /api/v1/me` renvoie rôle, droits et type de nœud ; l'interface n'affiche que le permis, et
   chaque route vérifie le droit côté serveur. Une seule interface, sans vues par rôle.
   `Role`, `Permission`, `NodeKind` et la table des droits vivent dans un crate partagé entre le
   backend et le frontend.
7. **Worker à interface** : même binaire, même interface, source de données différente. Le worker
   n'a pas de base de comptes ni de tâches : il interroge la centrale avec la clé de son nœud, par une
   liste de routes autorisées (jamais un proxy générique), chacune associée à une route de la centrale
   limitée au nœud. La centrale filtre par nœud ; le worker applique les droits de l'utilisateur.
   Première livraison en lecture seule : vue d'ensemble du nœud, ses tâches, ses outils installés,
   le catalogue de plugins. L'édition reste à la centrale.
8. **Connexion à un worker** : la centrale vérifie les identifiants quand elle est joignable
   (révocation et changement de mot de passe immédiats). Les comptes sont aussi copiés de la centrale
   vers les workers seulement (hachés Argon2id, versionnés, avec les suppressions) et ne servent que
   de secours si la centrale est injoignable. Les sessions du worker sont en mémoire ; elles sont
   purgées si le compte disparaît à la synchronisation.
9. **Secret de chiffrement** : fichier généré automatiquement dans `/config`, jamais affiché ;
   `SECRET_KEY_FILE` le remplace en option. Chaque valeur chiffrée porte un identifiant de clé et des
   données associées (plugin et champ) pour interdire son déplacement et préparer une rotation.

## Phases

1. Centrale : assistant, compte, sessions, secret automatique, clés d'API, crate partagé et droits.
2. Worker minimal : choix du rôle, serveur du worker, enrôlement, clé par nœud, statut et santé.
3. Vues du worker : vue d'ensemble, tâches, outils et plugins, avec connexion déléguée à la centrale.
4. Finition : tests, `openapi.yaml`, remise à zéro de l'installation de test.

## État d'avancement (13 nov. 2026)

**Phase 1 faite** (centrale : assistant, comptes Argon2id, sessions + CSRF, clés d'API, secret v2
automatique, crate `flux-shared`, interface de connexion, Réglages → compte et clés d'API).
Vérifiée en local, non déployée.

**Phase 2 faite** (code écrit ; tests Rust à exécuter par la CI, rien lancé en local) :
- `src/node_settings.rs` : le rôle worker est conservé dans `/config/node.json` (nom, adresse de la
  centrale) et la clé du nœud dans `/config/worker.key` (mode 0600). Sans fichier, le nœud démarre en
  centrale ; `coordinator` et `worker` restent acceptés comme arguments.
- Enrôlement : `POST /api/v1/nodes/enroll` (centrale, identifiants d'un administrateur, limités en essais)
  renvoie une fois une clé de rôle worker ; `POST /api/v1/setup/worker` (nœud non installé) l'appelle,
  conserve le rôle puis relance le processus en worker sans redémarrage du conteneur
  (`node::request_role_switch`). Une centrale installée ou un nœud déjà worker répond 409.
- Worker HTTP (`src/worker_http.rs`, même port 3000) : `/health`, `/api/v1/node/status`,
  `/api/v1/setup/status` ; la centrale expose aussi `/api/v1/node/status`. Un port occupé ne bloque pas
  le worker.
- Assistant : choix du rôle (centrale ou worker) puis formulaire d'enrôlement ; un worker enrôlé affiche
  un écran d'information en attendant les vues de la phase 3.
- Limites : le mot de passe administrateur transite vers l'adresse saisie sur la page d'installation
  (acceptable sur réseau de confiance, comme la fenêtre de réclamation) ; la redirection n'est pas suivie.

**Reste à faire :**
- Phase 3 : vues du worker en lecture seule (vue d'ensemble du nœud, tâches, outils, plugins) via des
  routes de la centrale limitées au nœud ; connexion déléguée à la centrale, comptes copiés
  (centrale → worker, versionnés, suppressions incluses) en secours ; sessions du worker en mémoire.
- Phase 4 : tests de la phase 3 et mise à jour de la documentation correspondante (`openapi.yaml` couvre déjà
  les routes de la phase 2).
- Déploiement : pousser `dev → test → main` (ADR 0002), approuver les runs `action_required`,
  remettre l'installation de test à zéro (supprimer `/config` après sauvegarde ; confirmer avant de
  toucher `/data`), recréer le compte par l'assistant, créer les clés worker dans Réglages, mettre à
  jour les workers (`WORKER_KEY_FILE` = clé d'API de rôle worker), redéployer l'app TrueNAS
  `flux-essai` et copier `frontend/dist/index.html` dans le volume.
- Pistes non prioritaires : récupération de mot de passe oublié, rôles supplémentaires, rotation de la
  clé de chiffrement (identifiant de clé déjà présent dans `enc:v2:`).

## Conséquences et limites

- Sans TLS, le mot de passe saisi sur une page d'installation ou de connexion circule en clair sur le
  réseau local : un reverse proxy TLS est recommandé hors réseau de test.
- Un worker non configuré est reconfigurable par le premier venu tant que son enrôlement n'est pas
  fait, comme la centrale.
- Des hachages de mots de passe sont copiés sur les workers : un worker compromis permettrait de les
  attaquer hors ligne. Argon2id et la copie réservée aux nœuds enrôlés en limitent le risque.
- Si la centrale est injoignable, un worker ne présente que ses données locales et refuse les
  connexions d'un compte inconnu de sa copie.
