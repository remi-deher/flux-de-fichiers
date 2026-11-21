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
- Variables intrinsèques en lecture seule : `file.*` (fichier de travail courant), `source.name`,
  `library.id`, `library.name`, `worker.name`, `job.id`, `flow.id`, et pendant le flux d'échec
  seulement `error.code` et `error.message`.
- Variables personnalisées `vars.<nom>` : écrites par le bloc « Définir une variable » ou par la sortie
  `variables` d'un script ; 64 au plus, scalaires, 4 Kio par valeur. Elles suivent la tâche lors d'un relais.
- Les conditions lisent les mêmes variables (propriété `vars.nom`, `file.ext`, `error.code`…).
- Code : `shared/src/flows/variables.rs`, `src/flows/substitution.rs`, `src/flows/variables_step.rs`.

### 2. Flux d'échec

- Un flux désigne son flux d'échec par la clé optionnelle `failureFlowId` de sa définition. Les
  définitions déjà stockées n'ont pas cette clé : rien ne change pour elles.
- Un flux d'échec global (`GET`/`PUT /api/v1/flows/failure-default`, réglage d'instance
  `default_failure_flow_id`) s'applique aux tâches dont le flux n'en désigne aucun, y compris sans flux.
- À la création de la tâche, la version courante du flux d'échec est figée dans `job_linked_flows`
  (migration 0038, rôle `failure`), comme celle du flux principal. Une relance d'une version précise
  refige les flux liés.
- Quand l'exécution échoue sur le worker, le flux d'échec s'exécute sur la même tâche, à partir de la
  source, avec `error.code` et `error.message`. Ses étapes et une étape de synthèse `failure-flow` sont
  ajoutées au journal. **La tâche reste échouée.**
- Pas de boucle : le flux d'échec d'un flux d'échec n'est jamais exécuté, et l'échec du flux d'échec est
  seulement journalisé.
- Limite : seuls les échecs d'exécution du flux sur le worker déclenchent le flux d'échec ; une perte de
  bail ou un refus côté central ne le déclenchent pas.

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

## Conséquences

- Nouvelle table additive `job_linked_flows` ; aucune migration de données.
- Le bail grossit des définitions liées ; seules celles réellement atteignables sont figées.
- Le budget d'étapes d'une exécution augmente du nombre de blocs de chaque sous-flux appelé.
- Interface : choix du flux d'échec (propre et global) dans les réglages du flux, choix du sous-flux
  (champ de type `flow`) dans l'inspecteur.

## Alternatives écartées

- Résoudre la dernière version des sous-flux à l'exécution : contraire à l'ADR 0012, une modification
  changerait des tâches déjà créées.
- Recopier le sous-flux dans le flux appelant à l'enregistrement : perd la réutilisation et la traçabilité.
- Relancer automatiquement la tâche depuis le flux d'échec : hors périmètre, la tâche reste échouée.
