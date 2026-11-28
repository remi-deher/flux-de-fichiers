# 0035 — Adresses : onglets et tiroirs

Statut : proposé. Lot 2 de la refonte de l'interface (proposition du 26 nov. 2026). Complète l'ADR 0014
(écrans du client) et remplace la mémorisation de l'onglet des Librairies (`LAST_TAB`, ADR 0030).

## Contexte

L'onglet d'un écran et le tiroir ouvert n'étaient que des signaux locaux : un lien ne rouvrait pas le même
écran, Retour quittait l'écran au lieu de fermer le tiroir, et l'onglet des Librairies était mémorisé dans
une variable globale pour survivre au passage par l'éditeur de script.

## Décision

1. **Onglet = second segment du chemin.** `/app/plugins/installed`, `/app/jobs/logs`, `/app/settings/keys`,
   `/app/flows/scripts`. L'onglet par défaut n'écrit aucun segment ; un nom inconnu ou un onglet non
   autorisé mène à l'onglet par défaut. Chaque écran à onglets déclare ses noms (`UrlTab`) et appelle
   `use_tab` : le signal d'onglet suit l'adresse et l'adresse suit le signal. Changer d'onglet ajoute une
   entrée d'historique et ferme le tiroir.
2. **Tiroir = paramètre de requête.** Détail : `?voir=<type>.<id>` ; création : `?nouveau=<type>`. `<type>`
   est l'énumération `DrawerKind` (Serde), jamais une chaîne libre. `use_detail_drawer` et
   `use_create_drawer` renvoient les signaux (`RwSignal<bool>` d'ouverture, identifiant sélectionné) que
   `Drawer` consomme déjà : le composant ne change pas.
3. **Historique.** Ouvrir un tiroir depuis une adresse sans requête pousse une entrée (Retour le ferme) ;
   fermer un tiroir ou passer de l'un à l'autre remplace l'adresse.
4. **`Location`.** `client/src/location.rs` lit et écrit l'adresse complète (écran, onglet, requête), sans
   navigateur : analyse, rendu et aller-retour sont testés. Le `Router` porte l'onglet et la requête.

## Migration

- Fait au lot 2 : les quatre écrans à onglets (Bibliothèques, Extensions, Activité, Paramètres) ; les tiroirs
  de création (mot de passe, clé d'API, compte, worker, dépôt, pack, ressource) et de détail (worker, flux,
  modèle de flux).
- Fait au lot 2b : les tiroirs de détail des Extensions (`extension`, `pack`, `outil`, `element`), du dépôt
  (`depot`), de la bibliothèque (`bibliotheque`) et du pack local (`pack-local`) ; l'onglet du tiroir
  (`?onglet=<n>`, rang de l'onglet, le premier n'écrit rien). Ouvrir, changer ou fermer un tiroir retire
  `onglet`. Une pièce du catalogue (`element`) s'ouvre dès que le catalogue est chargé.
- Fait au lot 3 : l'écran Flux a trois onglets, `/app/flows` (Flux, par défaut), `/app/flows/modeles` et
  `/app/flows/scripts` (`FlowsTab`, `client/src/flows/screen.rs`). Les onglets passent avant l'identifiant d'un
  flux dans `Route::parse` : un flux nommé `flux`, `modeles` ou `scripts` serait masqué (les identifiants
  générés portent un suffixe). Les Bibliothèques perdent « Flux & Modèles » et « Scripts » ; l'éditeur de script
  garde ses routes `/app/libraries/scripts/…` jusqu'au lot 4 mais appartient à l'entrée Flux et y revient.
  Le composant `PageSubnav` (`client/src/ui/page_subnav.rs`) porte la rangée d'onglets des écrans migrés.
- Fait au lot 4 : un seul éditeur de script (ADR 0030). L'inspecteur du bloc Script n'a plus de zone de code :
  il choisit un script, ouvre « Modifier le script » ou « + Nouveau script » dans l'éditeur, et garde les
  variables et le routage. Un ancien bloc à code direct (`mode: inline`, toujours exécuté par le moteur) se lit
  dans l'inspecteur et s'enregistre en script. Les routes de l'éditeur deviennent `/app/flows/scripts/new` et
  `/app/flows/scripts/<id>` (les anciennes `/app/libraries/scripts/…` restent lues). Ouvert depuis un flux,
  l'éditeur porte `?depuis=<flux>` et y revient à sa fermeture (`libraries/script_origin.rs`) ; la garde de
  sortie demande de confirmer si le flux n'est pas enregistré. L'éditeur n'est donc pas une couche au-dessus du
  flux : c'est un écran, avec retour au flux.
- Fait au lot 5 : les Bibliothèques ne montrent plus que les dossiers de médias surveillés (plus d'onglets).
  Les ressources, modèles de blocs et packs locaux vont dans l'écran Atelier, `/app/workshop`
  (`WorkshopPage`, `client/src/libraries/workshop.rs`), onglets `resources` (par défaut) et `packs`, entrée de
  navigation « Atelier » (`vocabulary::WORKSHOP`) dans le groupe « Traitements », avec le droit de lire les
  bibliothèques. `/app/libraries/resources` et `/app/libraries/packs` mènent à l'Atelier. Le bouton « Atelier »
  de la liste des flux devient « Éditer », pour ne pas confondre les deux mots.
- Reste : les tiroirs des modèles de blocs et du nouveau flux ; la garde de sortie (`unsaved`) des tiroirs
  de formulaire. Le tiroir de l'Activité garde ses paramètres actuels (`?job=`, `?step=`, `jobs/query.rs`).
- Un identifiant introuvable ouvre un tiroir vide ; le message « n'existe plus » reste à écrire.
