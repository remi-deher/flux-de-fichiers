# 0025 — Variables de script, exposition par les plugins et dépendances

Statut : accepté par l'utilisateur (21 nov. 2026) ; phase A en cours d'implémentation.

## Contexte

Un script reçoit sur `stdin` un JSON (`workingFile`, `sourceFile`, `workDir`, `mediaInfo`,
`config`). `config` fusionne les valeurs par défaut de l'opération (`defaults`) et la
configuration du nœud dans le flux (`merge_params`) : aucun schéma ne dit quelles clés existent,
de quel type elles sont, ni comment les saisir. Un script ne peut pas non plus s'appuyer sur ce
qu'un autre plugin sait (par exemple le codec d'un fichier, calculé par MediaInfo) sans
réimplémenter ce travail.

## Décisions

1. **Schéma dans le manifeste.** Un script déclare `runtime.variables` dans son opération : une
   liste de variables typées. Chaque variable a un nom (`[a-z][a-z0-9_]*`, 64 caractères au plus,
   le point est réservé aux variables exposées par les plugins), un libellé, une description,
   `required` et un type parmi `text`, `multiline`, `number` (`min`, `max`), `boolean`,
   `choice` (`options`) et `secret`. Une valeur par défaut doit correspondre au type ; un secret
   n'a jamais de valeur par défaut, est chiffré comme les secrets de plugins (ADR 0011) et n'est
   jamais affiché dans un aperçu. Le contrat est défini une fois dans `shared/src/script_variables.rs`
   et validé à l'écriture (central) et à la synchronisation d'un dépôt.
2. **Contrat `stdin` inchangé.** Le script lit les valeurs dans `config`. Aucune substitution dans
   le code du script, aucune évaluation shell : un aperçu « résolu » montre le JSON `config`
   que le script recevra, jamais une réécriture du code.
3. **Références dynamiques.** Une valeur peut contenir `{{référence}}`, résolue côté central ou
   worker avant d'écrire `config` : `workingFile`, `sourceFile`, `workDir`, `mediaInfo.…`,
   `nodes.<id>.message|nextPort|workingFile` et les variables exposées par les plugins. Une
   valeur qui est entièrement une référence garde son type JSON ; insérée dans un texte, elle
   devient une chaîne. Une référence inconnue est refusée à l'enregistrement du flux ; sans valeur
   à l'exécution, l'étape échoue avec un message clair, sauf si la variable a une valeur par défaut.
4. **Exposition par les plugins.** Le manifeste d'un plugin déclare `exposes` : des variables
   préfixées par son identifiant (`mediainfo.video.codec`), typées, avec un chemin dans le résultat
   de l'opération du plugin (jamais du code arbitraire) et une version de contrat. Un renommage,
   un retrait ou un changement de type impose une montée de version majeure du contrat. Une variable
   exposée n'a de valeur que si le plugin fournisseur la produit en amont dans le flux ; le
   sélecteur du nœud ne propose que celles disponibles à cet endroit. Seuls les plugins du
   catalogue exposent des variables ; l'ouverture aux scripts est reportée.
5. **Dépendance dure par défaut.** Un script déclare `requires` : les variables exposées qu'il
   consomme, avec le plugin fournisseur, soit dans le même dépôt, soit dans un autre dépôt désigné
   par son identifiant et l'URL de son index. Sans le fournisseur, le script est indisponible
   (sauf variable à valeur par défaut). L'installation affiche les dépendances manquantes et
   propose de les installer ; ajouter un dépôt tiers n'est jamais automatique et demande une
   confirmation explicite (nom du dépôt, officiel ou tiers).
6. **Option « sans dépendance ».** Un script peut être intégré sans ses dépendances : les
   variables sont alors saisies à la main, il est marqué « dépendances non vérifiées », le flux ne
   peut pas valider ces références à l'enregistrement et l'interface l'affiche clairement.
7. **Contrôles.** La CI du dépôt de plugins vérifie le schéma, le préfixe, la validité des types,
   la montée de version sur changement cassant et, quand une fixture existe, la résolution de chaque
   chemin exposé. Le central ne se fie pas à cette CI : il revalide à la synchronisation et ignore
   avec un avertissement toute fiche invalide (comme `skipped` et `warnings`, ADR 0010 et 0023).
   Pour le dépôt officiel, la revue de PR du gestionnaire valide ; un dépôt tiers reste « non vérifié ».

## Phasage

- **A** — Schéma `variables` du script, validation, lecture et écriture des scripts locaux, tableau
  des variables dans le tiroir d'un dépôt et déclaration dans l'éditeur de scripts locaux.
- **B** — Résolution des références à l'exécution (flux, sorties de nœuds), tests moteur.
- **C** — `exposes` des plugins, `requires` des scripts, dépendances et revalidation à la synchro.
- **D** — Formulaire du nœud généré depuis le schéma, sélecteur de références filtré, aperçu `config` résolu.

## Conséquences

- Les valeurs par défaut du schéma n'alimentent `config` qu'à partir de la phase B ; la phase A
  décrit et affiche seulement les variables.
- Les scripts existants, sans `variables`, restent valides (liste vide par défaut).
