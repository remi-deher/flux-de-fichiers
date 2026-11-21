# 0025 — Variables de script, exposition par les plugins et dépendances

Statut : accepté par l'utilisateur (21 nov. 2026) ; phases A à D implémentées.

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
4. **Exposition par les plugins.** Le contrat de l'opération d'un plugin déclare `exposes`
   (et non son manifeste : les workers ne reçoivent que les opérations, dans le claim, et jamais le
   catalogue). `exposes` porte un `namespace` (`[a-z][a-z0-9_]*`, jamais `nodes`), une
   `contractVersion` (`MAJEUR.MINEUR.CORRECTIF`) et des variables typées (jamais de secret) : un nom
   relatif (`video.codec`) et la requête de sa valeur dans `mediaInfo` après l'opération du plugin,
   jamais du code arbitraire. La requête est celle des conditions du flux : clés pointées, `[n]`,
   `[*]` et `[clé=valeur]` (`mediainfo.media.track[@type=Video].Format`) ; la première valeur non
   nulle est retenue, et un chemin pointé à indices (`a.0.b`) reste lu. Un sélecteur est préférable à
   un indice : l'ordre des pistes d'un fichier varie. La référence complète est `namespace.nom`
   (`{{mediainfo.video.codec}}`) ; le préfixe est imposé par construction, un plugin ne peut exposer
   que dans son espace de noms. Un nom ne peut pas être à la fois une valeur et un groupe
   (`video` et `video.codec`). Un renommage, un retrait ou un changement de type impose une montée
   de version majeure. Une variable n'a de valeur que si le plugin fournisseur l'a produite en amont
   dans le flux ; si deux opérations réclament le même espace de noms, la première (par identifiant
   d'opération) l'emporte. Seuls les plugins du catalogue exposent ; l'ouverture aux scripts est reportée.
5. **Dépendance dure par défaut.** Un script déclare `runtime.requires` : pour chaque variable
   consommée, sa référence complète, l'identifiant du plugin fournisseur, la version majeure du
   contrat visée (1 par défaut) et, si le fournisseur n'est pas dans le même dépôt, un pointeur
   `repository` (identifiant et URL d'index). Sans le fournisseur chargé, l'étape échoue avant
   l'exécution en nommant les variables manquantes. La résolution des dépendances d'un script
   planifie l'installation du fournisseur et signale un fournisseur absent avec le dépôt à ajouter ;
   ajouter un dépôt tiers n'est jamais automatique et reste une confirmation explicite.
6. **Option « sans dépendance ».** Un script peut être intégré sans ses dépendances
   (`runtime.dependenciesUnverified`) : les variables sont alors saisies à la main, il est marqué
   « dépendances non vérifiées », aucune dépendance n'est contrôlée ni installée, et l'aperçu
   l'affiche clairement.
7. **Contrôles.** La CI du dépôt de plugins vérifie le schéma, l'espace de noms, la validité des
   types, la montée de version sur changement cassant et, quand une fixture existe, la résolution de
   chaque chemin exposé. Le central ne se fie pas à cette CI : à la synchronisation il retire de la
   copie tout contrat d'opération dont `variables`, `exposes` ou `requires` sont invalides et le
   signale dans `SyncReport.skipped` (comme les icônes, ADR 0010 et 0024) ; au chargement, le
   registre refuse le même contrat (`register_json`), y compris pour un worker qui le reçoit dans un claim.
   Pour le dépôt officiel, la revue de PR du gestionnaire valide ; un dépôt tiers reste « non vérifié ».

## Phasage

- **A** — Schéma `variables` du script, validation, lecture et écriture des scripts locaux, tableau
  des variables dans le tiroir d'un dépôt et déclaration dans l'éditeur de scripts locaux.
- **B** — Résolution des références à l'exécution (flux, sorties de nœuds), tests moteur.
- **C** — `exposes` des plugins, `requires` des scripts, dépendances et revalidation à la synchro.
- **D** — Formulaire du nœud généré depuis le schéma, sélecteur de références filtré, aperçu `config` résolu.

## Conséquences

- Phase B (`src/flows/variables.rs`) : avant chaque script, les valeurs par défaut du schéma comblent
  les clés absentes du nœud, les références `{{…}}` sont résolues, puis chaque valeur est contrôlée
  et convertie selon son type (nombre, booléen, choix, bornes, `required`). Seules les clés
  déclarées sont traitées : le code en ligne et les autres clés du nœud ne sont jamais modifiés.
  Un `{{…}}` dont le contenu n'est pas un chemin valide reste littéral. Une référence sans valeur
  retombe sur le défaut de la variable, sinon l'étape échoue en nommant la variable.
- Les sorties des nœuds (`nodes.<id>.…`) ne viennent que des étapes exécutées par ce worker : après
  un relais de tâche (ADR 0018), un nœud exécuté avant le relais n'a plus de sortie.
- Phase C : `exposes` (contrat d'opération), `requires` et `dependenciesUnverified` (opération du
  script), validés à la synchronisation et au chargement ; le moteur expose chaque espace de noms
  dans les références et contrôle les dépendances avant le script ; l'aperçu d'un script de dépôt
  affiche ses dépendances et leur disponibilité ; la résolution des dépendances et `DependencyInstallResult`
  signalent le dépôt à ajouter (`suggestedRepositories`).
- Phase D : `POST /api/v1/flows/node-variables` renvoie, pour un nœud du graphe courant (même non
  enregistré), le schéma de son script, les références disponibles à cet endroit (flux, champs d'un
  exemple de métadonnées, sorties des nœuds en amont, variables des plugins qui s'exécutent avant,
  sondes comprises) et la `config` résolue avec des valeurs d'exemple (les sorties des nœuds en amont
  y sont des marqueurs). L'inspecteur du nœud génère le formulaire depuis le schéma, avec un sélecteur
  de références et cet aperçu. `PUT /api/v1/flows/{id}` refuse (400, `invalid_flow_variables`, avec la
  liste des références en cause) un flux dont une variable déclarée contient une référence inconnue ou
  vers un nœud qui ne le précède pas ; `POST /api/v1/flows/validate` la signale dans `errors`. Seules
  les clés du schéma sont contrôlées.
- Compatibilité des chemins à sélecteur : la version 0.70.0 n'acceptait que des chemins pointés. Une
  installation plus ancienne qui synchroniserait un catalogue dont `exposes` en contient écarterait
  toute l'opération (revalidation du central) ; un catalogue ne doit donc les utiliser qu'une fois les
  installations à jour.
- Fiche d'un plugin : `GET /api/v1/plugins/{id}` renvoie `exposures` (espace de noms, version du contrat,
  variables et chemin lu), affichées dans l'onglet Informations avec la référence `{{espace.nom}}` ;
  `GET /api/v1/plugins/exposed-variables` liste toutes les expositions du catalogue. Chacune porte `active`
  et `conflictsWith` : à espace de noms égal, la première (par identifiant d'opération) est lue par le
  moteur, les autres sont signalées comme contestées.
- Reportés : la résolution et la saisie des secrets dans le flux et la saisie de `requires` dans
  l'éditeur de scripts locaux.
- Les scripts existants, sans `variables`, restent valides (liste vide par défaut).
