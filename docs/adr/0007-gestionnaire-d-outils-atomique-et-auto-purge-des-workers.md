# 0007 — Gestionnaire d'outils atomique sans verrous et auto-purge de résilience des workers

Statut : accepté par l'utilisateur le 11 novembre 2026.

## Contexte et limites du modèle de « cache » initial

L'ADR 0004 et les premières implémentations décrivaient un mécanisme désigné sous le terme de « cache d'outils » avec gestion de verrous physiques `.lock` sur disque. L'usage réel et l'analyse technique ont mis en lumière trois incohérences majeures :

1. **Inadéquation du concept de « cache »** :
   Un cache au sens informatique est volatil, jetable et soumis à éviction (TTL, LRU) sans rompre le fonctionnement applicatif. Or, des outils tels que `mkvtoolnix`, `ffmpeg`, `mp4box` ou `mediainfo` sont des **dépendances d'exécution indispensables** (*Runtime Engines*). Les traiter comme un cache paresseux (téléchargement impromptu lors du premier traitement soumis) crée des latences imprévisibles et expose le traitement à des défaillances réseau non imputables au flux média.

2. **Fausse problématique de concurrence et risques des fichiers `.lock`** :
   - Chaque nœud (serveur coordinateur ou worker distant tel que TRUENAS ou ARTEMIS) dispose de son propre stockage local dédié. Les exécutables ne sont pas partagés en écriture sur un montage réseau distant.
   - Un outil est téléchargé et extrait **une seule fois par nœud**. Une fois en place, de multiples workers ou threads locaux l'exécutent simultanément en lecture seule (`execve`), ce qui ne génère aucune concurrence d'écriture.
   - L'usage de fichiers `.lock` sur disque introduit un risque sévère de verrous orphelins (*stale locks*) si le processus est interrompu brutalement (OOM killer, crash conteneur, coupure de courant), bloquant indéfiniment l'exécution sans nettoyage manuel.

3. **Accumulation d'artefacts temporaires orphelins lors des ruptures de service** :
   Lorsqu'un worker est arrêté ou déconnecté en plein travail de transcodage ou remuxage, des répertoires temporaires (`job-*`, `.tmp`) restent stockés sur le volume de travail (`work_dir` ou `staging`), saturant inutilement le stockage au fil du temps.

---

## Décisions retenues

### 1. Remplacement des verrous `.lock` par une installation atomique POSIX

- Toute notion de fichier `.lock` est définitivement éliminée du code source.
- L'installation d'un outil distribué s'effectue dans un répertoire temporaire unique :
  `tools/<tool_id>/<version>/.tmp-<arch>-<uuid>/`
- L'archive est vérifiée par empreinte SHA-256, décompactée en mémoire sans shell (`noShellUnpack: true`), le binaire reçoit les permissions Unix `0755` et son empreinte finale est validée.
- Une fois l'intégrité validée, le dossier temporaire est basculé vers son emplacement cible par l'appel système standard `fs::rename()`.
- Sous Linux / POSIX, `rename` est **strictement atomique**. Il élimine tout risque de fichier corrompu, ne nécessite aucun fichier de verrouillage et ne peut laisser aucun verrou orphelin.

### 2. Auto-purge de résilience au démarrage et à la reconnexion des workers

Pour garantir l'auto-guérison (*self-healing*) et l'absence totale de fichiers résiduels :
- **Au démarrage initial** : le worker local et les workers distants parcourent leur répertoire de travail (`work_dir` ou `staging`) et purgent immédiatement tout répertoire de job résiduel (`job-*`, staging non finalisé) ou fichier temporaire (`.tmp`).
- **À la reconnexion après rupture réseau** : si la connexion avec le coordinateur central est perdue puis rétablie, le worker effectue un balayage et nettoie son espace de travail temporaire avant d'accepter une nouvelle tâche (`claims`).
- Les dossiers d'outils et de configuration résidant dans ou à côté du répertoire de travail sont strictement préservés.

### 3. Coordination exclusive par l'API et la base de données (zéro verrou partagé)

- Les workers n'utilisent aucun système de fichiers partagé pour synchroniser leurs droits sur les fichiers sources ou cibles.
- L'autorité sur l'attribution des tâches appartient exclusivement à la base SQLite du coordinateur central via le mécanisme des baux et des jetons (`claims`).
- L'interface et les workers savent en temps réel quel agent traite quel fichier sans aucune manipulation de fichiers témoins sur disque.

### 4. Nettoyage de l'interface utilisateur

- Les composants d'instrumentation de démonstration (banc d'essai interactif du cycle de vie et inspecteur de contrats déclaratifs) sont retirés de l'interface Leptos.
- L'onglet *Plugins & Outils* présente uniquement les informations nécessaires à l'exploitation : catalogue des outils, état réel d'installation (`ready` ou `not_installed`), capacités, architectures supportées et liste des plugins déclarés.
