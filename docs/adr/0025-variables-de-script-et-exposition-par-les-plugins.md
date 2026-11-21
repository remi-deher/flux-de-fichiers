# 0025 — Variables de script, exposition par les plugins et dépendances

Statut : accepté par l'utilisateur (21 nov. 2026) ; phases A à D et lots 4 et 5 implémentés.

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
   dans le flux ; si deux plugins déclarent le même espace de noms, un script lit celui que désigne sa
   dépendance (`requires` : fournisseur et version majeure) ; sans dépendance, la première (par identifiant
   d'opération) l'emporte. Un script qui lie un même espace à deux fournisseurs ou à deux versions
   majeures est refusé. Seuls les plugins du catalogue exposent ; l'ouverture aux scripts est reportée.
   **Dépréciation.** Une variable exposée peut porter `deprecated` (message non vide, 300 caractères au
   plus) et `replacedBy` (nom relatif d'une autre variable du même contrat ; exige `deprecated`). Une
   variable dépréciée reste lue : un script qui l'utilise continue de fonctionner, et le central
   avertit sans bloquer (fiche du plugin, sélecteur de références, aperçu du nœud, `warnings` de la
   validation et en-tête `x-flux-warnings` à l'enregistrement), en citant le message et la variable de
   remplacement. Un plugin peut ainsi renommer une variable en gardant l'ancienne le temps d'une version.
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
- Les sorties des nœuds (`nodes.<id>.…`) viennent des étapes exécutées avant le nœud courant. Après
  un relais de tâche (ADR 0018), elles sont transmises au worker repreneur (voir « Sorties de nœuds
  et relais » plus bas).
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
- Liaison par la dépendance : à chaque étape, le moteur construit le contexte de références d'après le
  `requires` du script (`exposures_for_requirements`) : l'exposition du fournisseur et de la version
  majeure désignés, puis la première exposition de chaque espace de noms non lié. Deux scripts d'un même
  flux peuvent ainsi lire deux plugins qui partagent un espace de noms. Le sélecteur de références, le
  contrôle à l'enregistrement et l'aperçu suivent la même règle. `validate_requirements` refuse un
  espace de noms lié à deux fournisseurs ou à deux versions majeures.
- Dépréciation : `deprecated` et `replacedBy` sont validés comme le reste de `exposes` (au chargement et à la
  synchronisation). La CI du dépôt de plugins n'autorise le retrait d'une variable que si elle était déjà
  dépréciée dans la version de la base, ou avec une montée de version majeure du contrat.
- Dépendances d'un script local : `POST /api/v1/local/scripts` accepte `requires` et `dependenciesUnverified`
  (validés avec les mêmes règles que le catalogue, 400 sinon) et les écrit dans l'opération du script ; l'éditeur
  de scripts locaux les propose avec un choix parmi les variables exposées du catalogue
  (`GET /api/v1/plugins/exposed-variables`) ou une saisie manuelle, et la case « intégrer sans dépendance ».
- Secrets dans le flux (lot 4) : la valeur d'une variable `secret` n'est pas dans la clé du nœud mais
  dans `config.secrets.<nom>`, objet traité comme `config.custom_headers` des webhooks : chiffrée à
  l'enregistrement (`flow:<flux>:<nœud>:secret:<nom>` comme données associées, même clé et même format
  que les secrets de plugins), renvoyée par l'API avec une valeur vide (le nom seul indique qu'elle existe),
  conservée si elle est laissée vide, retirée si elle vaut `null`, re-chiffrée à la rotation de la clé,
  déchiffrée par le central au bail puis scellée pour le worker dans l'enveloppe du bail
  (`LeaseSecretsEnvelope.flow_headers`, emplacement `secret:<nom>`). Le chiffrement ne consulte aucun
  registre : il suit l'emplacement (`secrets`, `custom_headers`), jamais le schéma du script ; les noms
  `secrets` et `custom_headers` sont donc réservés et refusés pour une variable. Les flux existants
  n'ont aucun `secrets` : rien à migrer.
- Résolution d'un secret : valeur littérale, jamais de référence `{{…}}` ni de valeur par défaut ; le
  moteur la place sous le nom de la variable dans `config` (la clé `secrets` n'est pas transmise au
  script) et refuse un secret encore chiffré. Une variable `required` sans secret échoue l'étape. Le
  script reçoit le secret en clair dans `config` (contrat inchangé) mais le moteur retire sa valeur
  du message, de l'extrait de journal et des lignes de progression du script (remplacée par `••••••••`).
- Aperçu : `POST /api/v1/flows/node-variables` remplace tout secret présent dans `config.secrets`
  (enregistré ou saisi) par `••••••••` et le considère comme fourni ; il ne renvoie jamais la valeur. L'éditeur
  affiche un champ mot de passe jamais pré-rempli, « secret enregistré » et un bouton « Effacer ».
- Sorties de nœuds et relais (lot 5) : `RelayRequest.nodeOutputs` (message et port de sortie de chaque
  nœud exécuté, 2000 caractères de message au plus, 256 Kio au total sinon 400) est stocké avec le
  relais (`job_relays.node_outputs_json`, migration 0042) et renvoyé dans le claim du repreneur
  (`resumedOutputs`) ; le moteur les restaure avant de reprendre au nœud désigné. Le fichier de travail d'un
  nœud antérieur appartient au worker émetteur : `nodes.<id>.workingFile` n'a plus de valeur après un relais
  (la référence échoue avec son message habituel, sauf défaut de la variable). La reprise sans artefact
  (la tâche repart de la source) ne restaure rien, comme elle ne reprend pas le nœud.
- Les scripts existants, sans `variables`, restent valides (liste vide par défaut).
