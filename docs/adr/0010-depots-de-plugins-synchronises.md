# 0010 — Dépôts de plugins synchronisés, installation répartie et secrets chiffrés

Statut : accepté par l'utilisateur (12 nov. 2026).

## Contexte

Le catalogue provenait d'un catalogue codé en dur dans le binaire, la synchronisation d'un
dépôt ne faisait qu'écrire une date, aucun chemin n'installait un outil, et les clés d'API
des plugins étaient stockées en clair. Les jobs sur fichier exigent pourtant une sonde
installée dans le cache du nœud (ADR 0004, 0007).

## Décisions

1. **Rien n'est embarqué dans le binaire** : le catalogue vient uniquement des dépôts.
2. **Un dossier par dépôt** : `/config/plugins/<id-du-dépôt>/` reprend l'arborescence du
   dépôt (`catalogue/index.json`, `apps/…`, `plugins/…`). Les identifiants de dépôt sont des
   noms de dossier sûrs (`[A-Za-z0-9._-]`, 64 caractères, sans point initial). `PLUGINS_DIR`
   reste un dossier local de développement, prioritaire.
3. **Schéma de référence** : type `app` et dossier `apps/` (branche `dev` du dépôt de
   plugins). Une fiche inconnue est ignorée et journalisée sans invalider l'index.
4. **Synchronisation** : au démarrage, toutes les 6 h et à la demande (`POST …/sync`).
   L'URL d'index finit par `/catalogue/index.json`. Sans redirection, adresses privées
   refusées (sauf `PLUGIN_ALLOW_PRIVATE_HOSTS=1`), DNS figé sur des adresses contrôlées,
   fichiers limités à 1 Mio et 500 fichiers. La copie est écrite dans un dossier temporaire
   puis substituée d'un renommage ; un échec conserve l'ancienne copie.
5. **Installation répartie** : chaque nœud installe lui-même ses outils depuis le contrat
   `apps/<outil>/distribution.v1.json`, que les workers distants demandent au coordinateur
   (`GET /api/v1/worker/tools/{id}/distribution`, clé worker). L'archive est téléchargée en
   https (redirections revérifiées), son SHA-256 et celui du binaire sont contrôlés avant
   exécution, et l'extraction refuse tout lien ou type spécial. L'installation se déclenche
   avant un job qui exige la sonde, et par `POST /api/v1/tools/{id}/install`.
6. **Secrets** : les champs `secret` (et tout nom contenant `key`, `token`, `secret`,
   `password`) sont chiffrés au repos (ChaCha20-Poly1305, clé `/config/secret_key` créée en
   0600). L'API ne les renvoie jamais (`isSet`) ; un champ laissé vide conserve la valeur.
   Un ancien enregistrement en clair est chiffré à sa première lecture.

## Conséquences

- Tant qu'aucun dépôt n'est synchronisé, le catalogue est vide et aucun job sur fichier ne
  démarre : le dépôt officiel doit être lisible (public) et publier de **vrais artefacts**
  (URL https, empreintes réelles). Les contrats actuels sont des maquettes
  (`releaseStatus: contract-only`).
- Le paquet `mkvtoolnix` reste dans l'image tant que ces artefacts n'existent pas.
- Limite connue : les en-têtes `customHeaders` des blocs webhook, stockés dans la définition
  d'un flux, ne sont pas chiffrés.
