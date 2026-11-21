# 0017 — Moteur générique : tout passe par les contrats du dépôt

Statut : accepté (21 nov. 2026), implémenté. Orientation et vocabulaire validés par l'utilisateur
(14 nov. 2026). Constaté dans le code : plus de `ensure_job_tools`, de constantes d'outils ni d'enum
`FlowBlockKind` fermé ; blocs lus depuis le catalogue du dépôt de plugins, runners génériques
(`flows/binary_runner.rs`, `flows/http_runner.rs`, `flows/script_runner.rs`), cache d'apps
(`tools_cache.rs`). Écart restant : le code d'erreur typé `app_unavailable` (point 4) n'existe pas ;
une app API non configurée ou en échec remonte `integration_failed` (ou `probe_unavailable` pour la
sonde).

## Contexte

L'ADR 0012 place l'encodage dans les plugins et l'image n'embarque aucun outil ; pourtant le
moteur garde en dur ce qu'il devrait lire dans le dépôt de plugins :

- les outils (`app.mkvtoolnix`, `tool.ffmpeg`, `tool.mp4box`), leurs versions et leurs binaires
  (`mkvmerge`, `MP4Box`, `ffmpeg`), installés systématiquement par `ensure_job_tools` ;
- l'argv de chaque outil, construit en Rust (`mp4box.rs`, `encode.rs`, `media.rs`) ;
- les types de blocs, un enum fermé `FlowBlockKind` (`forgearr-shared`) exécuté par un `match` ;
- Sonarr, Radarr et les webhooks (`flows/arr.rs`, `webhook.rs`) ; la sonde `mkvmerge -J`.

Le dépôt `flux-de-fichiers-plugins` définit déjà les contrats nécessaires : distribution d'un
outil (`tool-distribution`), opération (`operation`), adaptateur argv (`command-argv`), capture de
la console (`console-capture`), bloc (`block`), flow, preset et application. Le moteur ne les lit pas.

## Modèle validé par l'utilisateur

- Une **app** s'appuie soit sur un **binaire** (FFmpeg, MKVToolNix, MP4Box…), soit sur une **API**
  (Sonarr, Radarr…).
- Un **plugin** apporte des **blocs** qui font appel aux apps (convertir de l'audio, mettre à jour
  Sonarr…).
- Le **flux déclare impérativement ce dont il a besoin** ; le moteur l'installe. On peut aussi
  installer une app à la main, même si aucun flux n'en a besoin.
- **Rien n'est en dur** dans le moteur : tout passe par des contrats, pour le découpler.

## Décision

1. **Le moteur ne garde que ses primitives** : entrée et sortie de fichier, condition, versions de
   flux, baux, progression, secrets, et un **runner générique**. Plus aucun nom d'outil, de version,
   de binaire ni d'argv dans le code.
2. **Chaîne de contrats** : flux → blocs → opérations (plugin) → app. Le runner exécute une
   opération en lisant son adaptateur argv et sa capture de console ; une app API est appelée par
   une opération HTTP (contrat à écrire, voir « Écarts »).
3. **Besoins d'un flux** : calculés par fermeture de ce graphe (chaque bloc donne une opération,
   chaque opération une app et une version) et **figés avec le flux** à la création de la tâche
   (ADR 0012) ; ils voyagent dans le bail.
4. **Installation à la demande** : avant d'exécuter, le worker réalise les besoins du flux figé :
   - app **binaire** : installée depuis le dépôt dans son cache, pour son architecture ;
   - app **API** : ne s'« installe » pas, elle se **configure** (adresse, secret scellé) ; un besoin
     non configuré ou injoignable arrête la tâche avant tout travail, avec un code d'erreur typé
     (`app_unavailable`) et un message qui dit laquelle ;
   - la sauvegarde d'un flux et l'éditeur avertissent d'un besoin non satisfait ; l'installation
     ne se fait pas à la sauvegarde.
5. **Installation manuelle** : l'écran Plugins permet d'installer ou configurer toute app du
   catalogue, indépendamment des flux.
6. **Retraits** : les constantes d'outils, `ensure_job_tools` en dur, les constructeurs d'argv, les
   blocs Sonarr/Radarr/webhook du binaire et `FlowBlockKind` fermé disparaissent. Ne restent natifs
   que `file-input`, `file-output` et `condition`. `FlowBlockKind` devient l'identifiant d'un bloc
   du catalogue, et le champ `Other(String)` actuel devient la règle.
7. **Compatibilité** : les flux enregistrés gardent leurs identifiants de blocs ; une table de
   correspondance des anciens identifiants vers ceux du catalogue (déjà partielle dans
   `FlowBlockKind::parse`) est conservée pour les relire.

## Écarts à combler dans le dépôt de plugins

- Contrat d'**opération HTTP** pour les apps API (adresse de base, authentification par secret,
  requête et réponse typées) : `command-argv` ne couvre que les binaires.
- Contrat de **configuration d'une app API** (champs, secrets, test de connexion).
- Contrat de **sonde média** consommé par le moteur à la place de `mkvmerge -J` en dur.
- Les artefacts d'outils réels (encore `contract-only`) : prérequis de l'installation à la demande.

## Vocabulaire (décision de l'utilisateur, 14 nov. 2026)

| Concept | Mot dans l'interface | Identifiant des contrats et du code |
|---|---|---|
| Binaire ou API (FFmpeg, MKVToolNix, Sonarr…) | **Application** | `app`, avec un type `binary` ou `api` |
| Plugin | **Plugin** | `plugin` |
| Bloc | **Bloc** | `block` |
| Paquet qui regroupe applications, plugins, flows et éléments « Autre » (jamais des blocs, qui appartiennent à leur plugin) | **Pack** | `pack` |

Dans le dépôt de plugins, `tool` devient `app` (champ `kind`) et l'ancien `app` devient `pack` :
`tool.ffmpeg` → `app.ffmpeg`, `app.media` → `pack.media`, schémas et documentation renommés. Le
renommage précède la lecture des contrats par le moteur, pour ne changer les identifiants qu'une
fois (précédent : ADR 0013).

## Phasage

0. Renommage `tool` → `app`, `app` → `pack` dans le dépôt de plugins et dans les identifiants lus
   par le moteur.
1. Runner générique et contrats d'argv pour MP4Box et FFmpeg ; besoins d'un flux et installation à
   la demande des apps binaires ; retrait des constantes.
2. Contrats API ; Sonarr, Radarr et webhook en plugins ; sonde par contrat.
3. `FlowBlockKind` ouvert et migration des identifiants.
4. Écran Plugins : installer ou configurer à la main, état des besoins d'un flux.

## Amendement (décision utilisateur, 15 nov. 2026)

Le moteur **n'embarque aucun bloc** dans son binaire (`CATALOG` et `native_owned_catalog` supprimés) :
même les primitives de flux (`file-input`, `condition`, `webhook`, `file-output`) vivent en fiches
JSON dans le dépôt de plugins (`plugin.core`) et alimentent l'API `/api/v1/flows/blocks` via
`aliases` pour la rétrocompatibilité des graphes enregistrés.
