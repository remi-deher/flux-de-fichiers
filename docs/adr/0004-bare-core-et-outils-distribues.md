# 0004 — Moteur Bare Core, distribution d'outils par plugins (Option B) et socle d'exécution

Statut : accepté par l'utilisateur le 9 novembre 2026.

## Contexte et limites du modèle initial

Dans la conception d'origine (ADR 0001, ADR 0003), l'image de conteneur applicatif Debian Bookworm (`Dockerfile`) installait directement les outils de traitement média (`mkvtoolnix`, `curl`) via les dépôts de paquets du système hôte (`apt-get`).

Cette approche présentait cinq limites majeures :

1. **Couplage fort et rigidité des déploiements** : toute mise à jour, correction ou ajout d'un outil d'encodage/remuxage imposait de recompiler et redéployer l'intégralité des images de conteneur (coordinateur et workers), rendant impossible l'évolution indépendante des chaînes de traitement.
2. **Empreinte et surface d'attaque des images** : embarquer des chaînes d'outils complètes gonflait la taille des images de conteneurs (plusieurs centaines de mégaoctets) et augmentait la surface d'exposition aux vulnérabilités système (CVE liées aux bibliothèques C/C++ tierces).
3. **Hétérogénéité matérielle et architectures cibles** : le parc de machines (par exemple serveurs x86_64 et NAS ARM64 / aarch64 comme TRUENAS ou ARTEMIS) dispose de capacités très contrastées (présence ou absence de GPU, encodeurs matériels Intel QuickSync, NVENC ou VA-API) que des paquets Debian standards ne peuvent pas exploiter de manière optimale.
4. **Absence d'extensibilité modulaire** : le système ne permettait pas d'intégrer dynamiquement de nouveaux formats (FFmpeg, MediaInfo, etc.) ou des outils tiers sans modifier le binaire ou l'image Docker de base.
5. **Absence de télémétrie matérielle temps réel** : le coordinateur ne disposait pas de métriques fines (charge CPU, saturation RAM, présence GPU, espace disque disponible) pour ordonnancer intelligemment les tâches selon la charge réelle des nœuds.

## Décision utilisateur du 9 novembre 2026

L'utilisateur a acté le choix de l'**Option B : Moteur Bare Core et distribution d'outils par plugins**.

### 1. Moteur « Bare Core »

L'application `flux-de-fichiers` (coordinateur central et agents workers) est un **moteur nu (Bare Core)** :
- Aucun outil tiers de traitement média (`mkvtoolnix`, `ffmpeg`, etc.) n'est embarqué en dur dans l'image Docker applicative de production.
- Le runtime applicatif se concentre strictement sur ses responsabilités fondamentales : API REST/TLS, persistance et baux PostgreSQL, orchestration distribuée, gestion des transferts de flux, contrôle d'intégrité cryptographique et interface de pilotage Leptos.
- Le seul outil interne intégré au binaire est la **sonde système de télémétrie matérielle** (`tool.internal.telemetry`).

### 2. Outil interne unique : la sonde système de télémétrie

La sonde interne mesure l'état matériel et les capacités locales du nœud hôte :
- **CPU** : taux d'utilisation global, nombre de cœurs logiques/physiques, indicateurs de saturation.
- **RAM** : mémoire totale, utilisée, disponible et swap.
- **GPU et accélérateurs matériels** : détection non intrusive des nœuds de rendu (`/dev/dri/renderD*`, cartes graphiques dédiées, accélération matérielle disponible). En l'absence de GPU, la valeur est explicitement nulle (`gpu: null`).
- **Stockage** : espace total, utilisé et disponible sur les volumes sandbox autorisés (`DATA_DIR`, `WORK_DIR`, `CACHE_DIR`).

Cette télémétrie est transmise au coordinateur (à l'enregistrement et lors des heartbeats périodiques) pour alimenter un algorithme d'ordonnancement prédictif et adapté aux capacités des nœuds.

### 3. Distribution des outils sous forme de binaires autonomes (Option B)

Tous les outils tiers nécessaires aux chaînes de traitement sont modélisés et distribués via le catalogue `flux-de-fichiers-plugins` :
- Chaque outil fait l'objet d'un manifeste et d'un contrat déclaratif (`tool-distribution.v1.schema.json`) spécifiant pour chaque architecture (`x86_64`, `aarch64`) :
  - L'URL de téléchargement HTTPS de l'archive (ou son chemin en mode pré-amorcé hors ligne) ;
  - Le format d'archive (`tar.gz`, `tar.xz`, `zip`) ;
  - L'empreinte cryptographique obligatoire **SHA-256 de l'archive** ;
  - Le chemin relatif du binaire dans l'archive extraite et son empreinte **SHA-256 du binaire extrait** ;
  - Les permissions d'exécution Unix minimales (`0755`, propriétaire non-root `app`, `root-forbidden`).
- Les workers maintiennent un cache local immuable et versionné (`tools-cache/<tool_id>/<version>/<arch>/`).
- Avant toute exécution, le worker vérifie obligatoirement l'empreinte SHA-256 du binaire présent dans le cache. Tout écart de somme de contrôle provoque le rejet immédiat de l'outil et l'échec de la tâche.

### 4. Confinement strict de l'exécution

L'exécution des binaires distribués sur les workers respecte des garanties de sécurité strictes :
- **Aucun privilège root** : le processus s'exécute sous le compte applicatif dédié non-root (`UID 10001`, `USER app`).
- **Aucun accès au socket Docker** : le montage de `/var/run/docker.sock` est strictement prohibé sur les nœuds workers et le coordinateur.
- **Aucune interprétation shell** : les outils sont invoqués via des appels système directs de processus (`tokio::process::Command::new`), sans passage par `/bin/sh -c` ou `bash`. L'extraction d'archive utilise des bibliothèques de décompression natives dans le runtime, sans invocations shell externes (`noShellUnpack: true`).
- **Paramètres typés et chemins opaques** : les arguments passés aux binaires (`argv`) sont validés et assainis. Les chemins de fichiers sont strictement bornés aux répertoires de travail sandboxés (`/work`, `/data`).

### 5. Modèle d'ordonnancement des macro-blocs (opérations composites)

Pour les flux de traitement nécessitant une suite d'opérations (par exemple inspection -> remuxage -> contrôle de conformité), le système introduit la notion de **macro-bloc** :
- Un macro-bloc encapsule une séquence ordonnée d'étapes au sein d'une unique tâche distribuée sur un worker.
- Il évite la démultiplication de micro-tâches avec allers-retours réseaux et écritures SQL au coordinateur.
- Il préserve la clarté de l'interface visuelle des flux (présentation sous forme d'un bloc composite repliable).
- **Politique d'erreur stricte (`errorPolicy: stop`)** : tout échec d'une étape unitaire interrompt immédiatement le macro-bloc, consigne les sorties d'erreurs masquées et empêche toute étape ultérieure, sans compensation aléatoire.
- **Testabilité unitaire** : un macro-bloc constitue une unité de test déclarative et autonome, validable isolément avec des jeux de médias synthétiques.

## Conséquences

### Positives
- **Légèreté et portabilité des conteneurs** : l'image applicative `flux-de-fichiers` est drastiquement réduite, débarrassée des paquets médias lourds.
- **Mise à jour sans interruption applicative** : un nouvel outil ou une nouvelle version d'outil est déployé par ajout dans le catalogue, sans redémarrer ni reconstruire les conteneurs du central ou des workers.
- **Sécurité renforcée** : chaque artefact exécutable est authentifié cryptographiquement par SHA-256 et s'exécute sous un profil de privilèges minimal.
- **Planification intelligente** : le coordinateur tire parti des métriques de la sonde pour orienter les charges vers les nœuds les plus aptes (disponibilité mémoire, présence d'accélérateurs).

### Contraintes et mesures associées
- **Amorçage du cache** : le premier lancement d'un traitement sur un worker neuf implique le téléchargement de l'outil si celui-ci n'est pas pré-amorcé. En environnement réseau restreint, le mode hors-ligne (`offline-preseeded`) permet de provisionner le cache à l'avance.
- **Robustesse de l'extraction** : le décompactage direct par le runtime doit se prémunir rigoureusement contre les attaques de type traversée de répertoires (*Zip Slip*), ce qui est garanti par la validation stricte des chemins relatifs sans `..`.
- **Compatibilité ascendante** : les flux existants basés sur `mkvmerge` sont traduits de manière transparente vers le contrat d'outil distribué `tool.mkvtoolnix@88.0.0`.
