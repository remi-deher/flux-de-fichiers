# Conception technique de l'ordonnancement des macro-blocs

Date : 9 novembre 2026. Statut : spécification d'architecture technique (ADR 0004).

## 1. Contexte et problématique

Dans une chaîne de traitement multimédia complète, un traitement élémentaire ne se résume que rarement à un appel d'outil unique. Une opération de remuxage type comprend par exemple :
1. L'identification et l'inspection fine des flux du fichier source (`mkvmerge -J`) ;
2. La validation des pistes, chapitres et métadonnées ;
3. Le remuxage effectif avec application d'options spécifiques (`mkvmerge -o ...`) ;
4. Le calcul d'empreinte SHA-256 et la validation de conformité du conteneur résultant.

### Pourquoi le modèle de micro-tâches séparées était inadapté
Si chaque étape ci-dessus constituait une tâche indépendante gérée au niveau du coordinateur central :
- Les fichiers médias intermédiaires (pouvant peser plusieurs gigaoctets) devraient être retransférés par le réseau vers le stockage central entre chaque étape.
- Le coordinateur subirait une avalanche de transactions PostgreSQL de prise et renouvellement de baux (`Claim`, `Heartbeat`).
- L'utilisateur ferait face à un graphe de flow visuel surchargé et difficilement lisible dans l'interface Leptos.

### Solution : Le Macro-Bloc (Opération composite)
Le **macro-bloc** regroupe une séquence ordonnée d'opérations au sein d'une **seule et même tâche atomique exécutée sur le worker**.

---

## 2. Structure déclarative d'un macro-bloc

Le contrat d'un macro-bloc (conforme au schéma `macro-block.v1.schema.json` défini dans `flux-de-fichiers-plugins`) s'articule autour des sections suivantes :

```
┌──────────────────────────────────────────────────────────────────┐
│                     Macro-Bloc Composite                         │
│                                                                  │
│  [ Entrées / Sockets ]                                           │
│   ├── source_file : FileRef                                      │
│   └── audio_language : String                                    │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Étape 1 : inspect (app.mkvtoolnix@88.0.0)                 │  │
│  │  Argv: ["--output-charset", "UTF-8", "-J", "$source_file"]│  │
│  │  Sorties: inspection_json                                  │  │
│  └─────────────────────────────┬──────────────────────────────┘  │
│                                │ succès                          │
│                                ▼                                 │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Étape 2 : remux (app.mkvtoolnix@88.0.0)                   │  │
│  │  Argv: ["-o", "$scratch/out.mkv", "$source_file", ...]     │  │
│  │  Sorties: remuxed_file                                     │  │
│  └─────────────────────────────┬──────────────────────────────┘  │
│                                │ succès                          │
│                                ▼                                 │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Étape 3 : verify_hash (app.internal.telemetry ou hash)    │  │
│  │  Sorties: final_sha256, final_bytes                        │  │
│  └─────────────────────────────┬──────────────────────────────┘  │
│                                │ succès                          │
│  [ Sorties / Sockets ]         ▼                                 │
│   ├── target_file : FileRef ($scratch/out.mkv)                   │
│   └── output_sha256 : String                                     │
└──────────────────────────────────────────────────────────────────┘
```

### 2.1 Interface externe (Sockets)
- `inputs` : liste des paramètres requis (fichiers d'entrée opaques, valeurs typées scalaires : chaînes, booléens, entiers).
- `outputs` : liste des artefacts produits en cas de succès complet du macro-bloc (fichier de résultat, empreinte SHA-256, métadonnées extraites).

### 2.2 Séquence ordonnée d'étapes (`steps`)
Chaque étape définit :
- `id` : identifiant unique de l'étape au sein du macro-bloc (ex: `step-inspect`, `step-remux`).
- `operation` : référence vers l'opération d'outil requise (ex: `operation.mkvmerge.inspect@1.0.0`).
- `toolRequirement` : l'identifiant et la version de l'outil autonome correspondant (ex: `app.mkvtoolnix@88.0.0`).
- `bindings` : association explicite entre les arguments attendus par l'opération et les valeurs disponibles (soit issues des `inputs` globaux du macro-bloc, soit issues des sorties des étapes antérieures).

---

## 3. Modèle d'exécution sur le Worker

L'exécuteur de macro-bloc sur le worker orchestre le déroulement pas à pas :

### 3.1 Initialisation de la Scratch Sandbox
Pour chaque instance de tâche exécutant un macro-bloc :
1. Création d'un répertoire de travail isolé :
   `/work/jobs/{job_id}/macro_{macro_id}_{execution_id}/`
2. Sous-dossiers dédiés :
   - `input/` : liens en lecture seule ou copies des fichiers sources vérifiés.
   - `scratch/` : espace de travail pour les fichiers intermédiaires entre étapes.
   - `logs/` : journaux d'exécution et captures masquées de chaque étape.

### 3.2 Vérification préalable des outils
Avant de lancer la première étape :
- L'exécuteur résout l'ensemble des outils requis par toutes les étapes de la séquence.
- Il s'assure que chaque binaire d'outil est présent dans le cache local et valide son empreinte SHA-256.
- Si un outil manque, il déclenche son acquisition dans le cache worker (selon la procédure de l'ADR 0004). Si l'acquisition échoue, le macro-bloc est refusé avant même le début du traitement.

### 3.3 Exécution séquentielle et passage des données
Les étapes sont exécutées strictement dans l'ordre de la séquence déclarée :
- Aucune interprétation shell n'est permise. Les arguments (`argv`) sont résolus statiquement à partir des liaisons typées (`bindings`).
- Les chemins de fichiers sont résolus en chemins absolus stricts confinés dans la sandbox de la tâche.
- Les sorties scalaires (code de retour, JSON parsé) sont mémorisées dans un contexte d'exécution en mémoire (`ExecutionContext`).
- Les sorties de type fichier sont vérifiées sur disque à la fin de l'étape.

---

## 4. Politique de gestion des erreurs (`errorPolicy: "stop"`)

La robustesse et la prévisibilité sont assurées par une règle stricte :

### 4.1 Arrêt immédiat sans compensation aléatoire
- Si une étape échoue (code de sortie non nul, timeout dépassé, sortie invalide, format non reconnu) :
  1. **Interruption immédiate** : le moteur stoppe net l'exécution. Les étapes suivantes de la séquence sont ignorées et ne sont jamais exécutées.
  2. **Capture des diagnostics** : capture de `stderr` et des dernières lignes de `stdout` après masquage obligatoire des secrets et informations sensibles.
  3. **Rapport d'erreur structuré** : constitution d'un objet d'erreur détaillé :
     ```json
     {
       "macro_id": "macro.remux_and_verify",
       "failed_step_id": "step-remux",
       "error_kind": "NonZeroExitCode",
       "exit_code": 2,
       "error_message": "mkvmerge exited with error code 2",
       "masked_stderr": "Error: track 2 is corrupted at timestamp 00:04:12"
     }
     ```
  4. **Purge de la sandbox** : les fichiers intermédiaires volumineux dans `scratch/` sont nettoyés pour libérer l'espace disque.
  5. **Notification d'échec au coordinateur** : la tâche est marquée en échec (`POST /api/v1/jobs/{id}/failure`) avec le détail de l'étape fautive.

### 4.2 Masquage systématique des journaux
Avant toute sérialisation ou transmission au coordinateur :
- Filtre regex sur les motifs sensibles : clés secrètes d'agents, mots de passe, tokens de baux, chaînes de connexion PostgreSQL.
- Troncature stricte à un plafond maximal d'octets (ex. 64 Kio) pour éviter toute saturation des bases de données.

---

## 5. Testabilité unitaire déclarative (`unitTestSurface`)

Conformément à la spécification `unitTestSurface` :
- Un macro-bloc est conçu pour être testable comme une unité atomique autonome (`testableAsSingleUnit: true`).
- Les tests d'intégration (exécutés en CI distante conformément aux règles du projet) fournissent des jeux de fixtures synthétiques aux entrées du macro-bloc et vérifient les assertions sur les sorties finales sans dépendre de l'ensemble du graphe applicatif.
- Cette approche garantit que les opérations combinées peuvent être éprouvées unitairement avant toute mise en production dans un flow complexe.
