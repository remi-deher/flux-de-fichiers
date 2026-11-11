# ADR 0009 : Découplage strict des plugins — Exécution exclusive par blocs d'action dans les flux et exception d'inspection implicite

## Statut

Accepté par décision utilisateur du 11 novembre 2026.

## Contexte

Dans les architectures multimédias classiques, les intégrations avec des services tiers (tels que Sonarr, Radarr, des webhooks Discord/Slack ou des scripts personnalisés) sont fréquemment implémentées sous la forme de déclencheurs globaux automatiques (*hooks post-traitement*) rattachés au serveur.
Cette approche rigide présente des limites majeures :
- **Absence de découplage temporel** : la notification est systématiquement envoyée à la toute fin du cycle serveur, empêchant d'informer d'un démarrage de traitement, d'un jalon intermédiaire ou d'une bascule conditionnelle.
- **Effets de bord incontrôlés** : si un flux d'aiguillage conditionnel décide d'ignorer un fichier (ex: fichier non éligible) ou si un traitement est interrompu volontairement, un hook global aveugle risquerait de notifier prématurément ou à tort l'écosystème aval (*arr).
- **Confusion entre configuration et déclenchement** : la simple saisie d'une URL et d'une clé d'API dans les paramètres d'un plugin ne doit pas suffire à interférer avec le pipeline de traitement.

## Décisions

### 1. Principe de non-autonomie des plugins

- **Aucun plugin n'est autonome au niveau du serveur**. Un plugin ne s'exécute jamais de manière spontanée en tâche de fond ni en hook global implicite attaché au coordinateur.
- **L'exécution d'un plugin s'effectue exclusivement au travers d'un ou plusieurs blocs d'action disposés au sein d'un flux (Flow)**.
- Les écrans de réglages des plugins (table `plugin_settings` / panneau *Plugins*) servent uniquement de coffre de configuration (clés d'API, URLs cibles, correspondances de chemins locaux/distants). Ils fournissent les valeurs par défaut exploitées par les blocs du flux, sans jamais déclencher d'exécution par eux-mêmes.

### 2. Découplage temporel et passage de contexte d'état (`FlowContext`)

- Chaque bloc d'action de plugin positionné sur le plan de travail reçoit l'état exact du traitement au moment précis où le jeton d'exécution le traverse :
  - Identifiant unique de la tâche (`job_id`) ;
  - Fichier source et fichier de travail courant (`working_file`) ;
  - Métadonnées techniques du média (`media_info`) ;
  - Étape courante et statut à cet instant (`running`, `warning`, `step_completed`) ;
  - Horodatage ISO-8601.
- Grâce à ce découplage :
  - **En tête de flux** : un bloc Webhook / Notification peut émettre un avis de prise en charge (*"Début de traitement du média"*).
  - **Sur une branche conditionnelle** : un bloc peut alerter d'une déviation (*"Échec accélération GPU ➔ Bascule transcodeur CPU logiciel"*).
  - **En queue de flux (après sortie validée)** : un bloc Sonarr ou Radarr peut déclencher l'ordre de réanalyse (`RescanMovie`, `DownloadedEpisodesScan`) uniquement lorsque le fichier est définitivement finalisé et garanti intègre.

### 3. L'exception d'architecture assumée : L'inspection implicite initiale

- La seule exception assumée à la règle du bloc explicite est le **scan d'inspection initial** (MediaInfo / FFprobe).
- Ce scan n'est pas représenté par un bloc obligatoire préalable sur le flux afin de ne pas surcharger inutilement le graphe visuel.
- Cette exception est un **choix de conception justifié** : l'extraction des pistes (codecs, résolutions, débits, langues audio et sous-titres) est le prérequis indispensable pour initialiser le `FlowContext`. Elle est donc exécutée automatiquement en sous-main dès l'interception du fichier source, avant l'entrée dans le premier bloc du flux.

## Conséquences

- **Composabilité totale** : les pipelines deviennent pleinement modulaires et personnalisables à l'instar d'outils type Zapier ou FileFlows.
- **Fiabilité opérationnelle** : suppression totale des faux positifs et des rescans intempestifs sur les serveurs Sonarr/Radarr.
- **Testabilité accrue** : chaque bloc d'action de plugin peut être testé unitairement dans la suite de tests en lui passant un contexte synthétique sérialisé.
