# 0026 — Variables de flux, flux d'échec et sous-flux

Statut : proposé (21 nov. 2026). Implémenté sur la branche `feat/flux-variables-conditions-ordonnancement`,
non vérifié par la CI au moment de la rédaction. Précise l'[ADR 0012](0012-perimetre-perte-de-bail-et-versions-de-flows.md)
(versions figées à la création de la tâche) et l'[ADR 0018](0018-relais-de-tache-par-bloc.md) (relais).

## Contexte

Un flux est un graphe acyclique exécuté sur une tâche ; sa version est figée à la création de la tâche.
Trois besoins restaient sans réponse : transmettre une valeur d'un bloc à un autre, réagir à l'échec
d'une tâche (notifier, nettoyer) sans dupliquer ces blocs dans chaque flux, et réutiliser une suite de
blocs commune à plusieurs flux.

## Décision

### 1. Variables de flux (déjà implémentées)

- Une seule syntaxe pour tous les blocs (binaire, script, HTTP) : `{$expr}` est remplacé par la valeur,
  `{$expr|défaut}` utilise `défaut` si la valeur est absente ou vide, `{{$` produit le littéral `{$`.
  Une chaîne réduite à un seul jeton garde son type JSON. Le champ `code` d'un script n'est jamais rendu.
- Variables intrinsèques en lecture seule : `file.*` (fichier de travail courant, dont `file.path`),
  `source.name`, `source.path`, `work.dir`,
  `library.id`, `library.name`, `worker.name`, `job.id`, `flow.id`, et pendant le flux d'échec
  seulement `error.code` et `error.message`.
- Variables personnalisées `vars.<nom>` : écrites par le bloc « Définir une variable » ou par la sortie
  `variables` d'un script ; 64 au plus, scalaires, 4 Kio par valeur. Elles suivent la tâche lors d'un relais.
- Les conditions lisent les mêmes variables (propriété `vars.nom`, `file.ext`, `error.code`…).
- Code : `shared/src/flows/variables.rs`, `shared/src/flows/template_scope.rs`, `src/flows/substitution.rs`, `shared/src/flows/set_variable.rs`, `shared/src/flows/node_references.rs`.

### 2. Flux d'échec

- Un flux désigne son flux d'échec par la clé optionnelle `failureFlowId` de sa définition. Les
  définitions déjà stockées n'ont pas cette clé : rien ne change pour elles.
- Un flux d'échec global (`GET`/`PUT /api/v1/flows/failure-default`, réglage d'instance
  `default_failure_flow_id`) s'applique aux tâches dont le flux n'en désigne aucun, y compris sans flux.
- À la création de la tâche, la version courante du flux d'échec est figée dans `job_linked_flows`
  (migration 0046, rôle `failure`), comme celle du flux principal. Une relance d'une version précise
  refige les flux liés.
- Quand l'exécution échoue sur le worker, le flux d'échec s'exécute sur la même tâche, à partir de la
  source, avec `error.code` et `error.message`. Ses étapes et une étape de synthèse `failure-flow` sont
  ajoutées au journal. **La tâche reste échouée.**
- Pas de boucle : le flux d'échec d'un flux d'échec n'est jamais exécuté, et l'échec du flux d'échec est
  seulement journalisé.
- Échecs hors exécution sur le worker (perte de bail constatée par `claim`, refus de publication,
  `source_changed`, publication incertaine, aucun worker capable d'un relais, échec signalé par le
  worker avant ou après le flux) : la colonne `jobs.failure_flow_state` (migration 0048) passe à
  `pending` par un déclencheur SQLite, à chaque passage d'une tâche à `failed` qui a un flux d'échec
  figé. `fail` la fixe directement à `done` quand le journal contient déjà l'étape `failure-flow`
  (le worker l'a exécuté) : pas de seconde exécution.
- Le central exécute les flux en attente (`failure_flow_runner`, toujours actif, worker intégré ou
  non) : il passe la tâche à `done` **avant** l'exécution (au plus une exécution par échec, même
  après un redémarrage), exécute le flux d'échec figé avec `error.code` = `error.message` = code
  d'erreur de la tâche, sur une copie vérifiée de la source si le central la lit, sinon sur un
  fichier vide du même nom, puis ajoute ses étapes à la fin du journal. La tâche reste échouée.
- Une nouvelle tentative (réservation) remet l'état à NULL et retire la synthèse `failure-flow` de
  l'échec précédent.
- Limites restantes : le flux d'échec exécuté par le central utilise les outils et plugins du
  central (pas ceux d'un worker spécialisé) et ne voit pas une source détenue par un autre worker ;
  si un worker exécute le flux d'échec puis perd son bail avant d'enregistrer son journal, le
  central l'exécute une seconde fois.

### 3. Sous-flux

- Bloc primitif « Aller au sous-flux » (`essentiel.sub-flow`, primitive `sub-flow`, réglage `flowId`) :
  le moteur exécute le flux appelé à partir de son bloc d'entrée, sur le même fichier de travail et les
  mêmes variables, puis reprend à la sortie du bloc appelant (appel avec retour). Une sortie terminale ou
  l'absence de suite dans le sous-flux marque son retour.
- Les versions des sous-flux atteignables (depuis le flux principal et depuis le flux d'échec) sont
  figées dans `job_linked_flows` (rôle `sub_flow`) et voyagent avec le bail (`Claim.linked_flows`), avec
  leurs contrats, outils, réglages de plugins et en-têtes chiffrés.
- Profondeur maximale : `MAX_SUB_FLOW_DEPTH` = 4 appels imbriqués.
- Récursion refusée deux fois : à l'enregistrement (`check_sub_flow_links`, appelé par `PUT
  /api/v1/flows/{id}`, la restauration de version et `POST /api/v1/flows/validate`, qui refuse aussi un
  sous-flux ou un flux d'échec introuvable) et à l'exécution (flux déjà en cours d'appel).
- Un relais est refusé dans un sous-flux : le nœud de reprise ne saurait désigner un sous-flux.
- La classe de ressource CPU/GPU reste calculée sur le flux principal seulement.

### 4. Modèle unique `{$expr|défaut}`

Décision utilisateur : la syntaxe `{{référence}}` des variables de script (ADR 0025 §3) disparaît comme
dialecte. Tous les modèles (configuration des blocs, variables déclarées des scripts, défauts des
manifestes) s'écrivent `{$expr|défaut}` et passent par **une portée, un analyseur, un rendu et une passe**.

| Ancienne référence (ADR 0025) | Expression (ADR 0026) | Famille (`ScopeNamespace`) |
|---|---|---|
| `{{workingFile}}` | `{$file.path}` | intrinsèque |
| `{{sourceFile}}` | `{$source.path}` | intrinsèque |
| `{{workDir}}` | `{$work.dir}` | intrinsèque |
| `{{mediaInfo.<chemin>}}` | `{$media.<chemin>}` | métadonnées |
| `{{nodes.<id>.message}}`, `nextPort` | `{$nodes.<id>.message}`, `nextPort` | nœud |
| `{{nodes.<id>.workingFile}}` | `{$nodes.<id>.file}` | nœud |
| `{{<espace>.<nom>}}` (plugin) | `{$<espace>.<nom>}` | plugin |

- **Portée** : `TemplateScope` (`shared/src/flows/template_scope.rs`) ; `FlowVariables` l'implémente
  (intrinsèques, `vars.*`), `NodeScope` (`src/flows/variables.rs`) y ajoute `media.*`, `nodes.*` et les
  expositions des plugins. Le même `NodeScope` sert à tous les blocs d'une étape, et à l'aperçu.
- **Espaces réservés** : un plugin ne peut pas exposer sous `nodes`, `media`, `vars`, un espace de
  variable intrinsèque (`file`, `source`, `work`, `library`, `worker`, `job`, `flow`, `error`) ni un
  espace HTTP (`context`, `var`, `setting`, `config`). La liste est dérivée de `IntrinsicVariable::ALL`,
  `CUSTOM_NAMESPACE` et `HTTP_NAMESPACES` (`is_reserved_namespace`). Les conditions ne lisent toujours
  comme variables que les intrinsèques et `vars.*`.
- **Référence absente** :
  1. avec `|défaut`, le défaut s'applique quand la valeur est absente, nulle ou vide, partout ;
  2. sans défaut, dans une variable déclarée d'un script, une expression d'une famille connue
     (intrinsèque, `vars`, `media`, `nodes`, variable exposée déclarée) sans valeur fait échouer le bloc
     (`MissingReference`, message en `{$…}`), après repli sur le défaut du schéma ; `required` reste
     contrôlé ensuite ;
  3. ailleurs (autres clés, blocs binaires, HTTP, expression d'un espace inconnu) : texte vide.
     L'enregistrement refuse les expressions inconnues des variables déclarées, et l'éditeur les signale.
- **Types** : une chaîne réduite à un seul jeton garde le type JSON de sa valeur. Changement de
  comportement : dans un texte, `null` donne désormais un texte vide (l'ancien dialecte écrivait `null`).
- **Secrets et code** : `code` et `secrets` (`RAW_CONFIG_KEYS`) ne sont jamais rendus ni convertis ;
  un secret reste littéral et masqué dans l'aperçu. L'échappement `{{$` est conservé.
- **Une seule passe** : `resolve_node_config` part de la configuration brute du nœud ; la seconde passe
  sur une configuration déjà rendue est supprimée, donc plus d'injection de second ordre
  (`vars.x = "{$work.dir}"` puis `{$vars.x}` donne le texte `{$work.dir}`).
- **Blocs HTTP** : les jetons de l'exécuteur (`context.*`, `var.*`, `setting.*`, `config.*`) et les
  expressions inconnues de la portée restent tels quels pour qu'il les résolve.
- **Compatibilité** : une seule fonction convertit `{{x}}` en `{$x}`
  (`shared/src/flows/legacy_references.rs`), `x` étant traduit selon la table. Dans les configurations
  de nœuds (exécution, enregistrement, aperçu), seules les racines du moteur (`workingFile`,
  `sourceFile`, `workDir`, `mediaInfo.*`, `nodes.<id>.<champ>`) sont converties : un autre `{{…}}`
  (gabarit de l'outil appelé, moustache d'un corps JSON) reste littéral. Les valeurs et défauts des
  variables déclarées d'un script, seuls interprétés par l'ancien moteur, convertissent tout chemin ;
  leur rendu strict tient un texte vide pour une valeur et signale toute expression sans valeur ni
  défaut, même d'un espace de noms inconnu. Elle est appliquée à l'exécution (versions de flux figées,
  jamais réécrites en base ; sous-flux ; flux d'échec), aux défauts des manifestes installés, à l'aperçu,
  au simulateur et à la validation (qui avertit « ancienne syntaxe »). L'enregistrement d'un flux, la
  restauration d'une version, les scripts et modèles de flux locaux réécrivent au nouveau format.
- **Éditeur** : un seul sélecteur d'expressions (familles `ReferenceSource`, dont `variables`), alimenté
  par `POST /api/v1/flows/node-variables`.

## Conséquences

- Nouvelle table additive `job_linked_flows`, colonne `jobs.failure_flow_state` et déclencheur
  (migration 0048) ; aucune migration de données.
- Le bail grossit des définitions liées ; seules celles réellement atteignables sont figées.
- Le budget d'étapes d'une exécution augmente du nombre de blocs de chaque sous-flux appelé.
- Interface : choix du flux d'échec (propre et global) dans les réglages du flux, choix du sous-flux
  (champ de type `flow`) dans l'inspecteur.

## Alternatives écartées

- Résoudre la dernière version des sous-flux à l'exécution : contraire à l'ADR 0012, une modification
  changerait des tâches déjà créées.
- Recopier le sous-flux dans le flux appelant à l'enregistrement : perd la réutilisation et la traçabilité.
- Relancer automatiquement la tâche depuis le flux d'échec : hors périmètre, la tâche reste échouée.
