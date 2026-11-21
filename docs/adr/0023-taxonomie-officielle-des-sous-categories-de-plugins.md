# 0023 — Taxonomie officielle des sous-catégories de plugins

Statut : accepté par l'utilisateur (21 nov. 2026) ; implémenté.

## Contexte

Les plugins ont une catégorie fermée, propriété de Flux (`PluginCategory` : `media`, `data`,
`io`, `other`), distincte du type et de la capacité. Une valeur inconnue devient `other` ; les
mots libres vont dans les `tags` ([ADR 0009](0009-plugins-comme-blocs-d-action-et-inspection-implicite.md),
[ADR 0010](0010-depots-de-plugins-synchronises.md)).

Il manque un niveau intermédiaire pour regrouper des plugins proches (Plex, Jellyfin et Emby
sous « Serveurs média »). Les écrire en dur dans le code obligerait à publier une version de
Forgearr pour chaque nouvelle famille. Les laisser libres à chaque dépôt est dangereux : à
terme n'importe qui pourra créer un dépôt, et le catalogue se remplirait de variantes
(« Serveurs média », « Media Servers », « Media-Server ») qui fragmentent la navigation.

## Décisions

1. **Le vocabulaire appartient à Flux, pas aux dépôts.** Un dépôt *référence* une
   sous-catégorie par son identifiant ; il ne peut ni en créer une ni en fournir le libellé.
2. **Registre officiel versionné.** Un fichier de données unique,
   `shared/src/plugins/subcategories.v1.json`, versionné dans le dépôt Forgearr et embarqué à la
   compilation (le central et le client partagent le même). Il est désérialisé en types Serde
   stricts (champs inconnus refusés, version, identifiants uniques et libellés `fr` et `en`
   vérifiés) ; un registre invalide fait échouer le test de CI qui le charge. À l'exécution, un
   registre illisible équivaut à un registre vide : tout plugin retombe sur sa catégorie. Ce fichier est du vocabulaire de Flux,
   au même titre que l'enum `PluginCategory`, et non du contenu de catalogue : l'ADR 0010
   (« rien n'est embarqué dans le binaire ») ne s'y applique pas.
3. **Forme du registre.** Un champ `version` (entier) et une liste d'entrées :
   `id`, `category` (catégorie parente, valeur de `PluginCategory`), `labels` (une chaîne par
   langue, avec au moins `fr` et `en`), `order` (entier, tri croissant commun à toutes les catégories). Exemple :
   `{ "id": "media-servers", "category": "media", "labels": { "fr": "Serveurs média",
   "en": "Media servers" }, "order": 10 }`.
4. **Identifiant.** Type `SubcategoryId` (newtype validé) : minuscules, chiffres et tirets,
   64 caractères au plus, sans tiret en tête ni en fin. Aucune comparaison de libellés.
5. **Déclaration dans le manifeste.** Champ optionnel `subcategory` (identifiant). Aucun
   libellé n'est lu dans le manifeste d'un dépôt.
6. **Comportement 1 — repli silencieux.** Une sous-catégorie absente du registre, mal formée,
   ou rattachée à une catégorie parente différente de celle du plugin n'invalide ni le plugin
   ni l'index : le plugin reste dans sa catégorie, sans sous-groupe. C'est le même principe
   que `PluginCategory::from_label`.
7. **Comportement 2 — avertissement pour l'auteur du dépôt.** Le repli est journalisé
   localement (dépôt, plugin, valeur reçue, cause) et signalé dans l'écran de gestion du dépôt,
   pour que l'auteur sache quoi corriger. Un avertissement par plugin et par synchronisation (`SyncReport.warnings`, `RepositoryDetailView.warnings`).
8. **Pas d'espace de sous-catégories propre à un dépôt.** Une sous-catégorie non officielle
   n'est jamais affichée à côté des officielles. Cette option est écartée tant qu'aucun besoin
   réel n'existe ; la rouvrir demande un nouvel ADR.
9. **Évolution du registre.** Ajouter une sous-catégorie passe par une modification du fichier
   dans le dépôt Forgearr, relue comme toute modification de code. Un identifiant publié n'est
   jamais renommé ni réutilisé pour un autre sens ; un identifiant retiré fait retomber les
   plugins qui le citent dans le repli du point 6. Un changement de forme du fichier crée un
   `v2`.
10. **API et interface.** Le catalogue expose l'identifiant de la sous-catégorie retenue
    (`CatalogEntry.subcategory`, absent en cas de repli). Le client en tire le libellé du registre
    partagé (langue de l'interface, repli sur `en`) et regroupe sur deux niveaux, dans l'ordre du
    registre ; sans sous-catégorie, le plugin apparaît dans la grille de sa section.
11. **Catégorie du plugin obligatoire.** Une sous-catégorie n'est retenue que si le plugin
    déclare aussi la catégorie parente (manifeste, sinon index) ; sans catégorie déclarée, c'est
    un repli avec avertissement. La valeur du manifeste l'emporte sur celle de l'index, et les
    deux sont revalidées.

## Conséquences

- Schéma de bloc et `shared/src/plugins/vocab.rs` : champ `subcategory`, type `SubcategoryId`,
  chargement et validation du registre ; `src/plugins/mod.rs` : résolution et repli.
- Catalogue client (`client/src/plugins/catalog.rs`) : regroupement sur deux niveaux, libellés
  localisés, composant de groupe réutilisable (pas de balisage dupliqué).
- Tests Rust natifs : validation de `SubcategoryId`, registre valide et registre invalide,
  repli pour identifiant inconnu, mal formé ou rattaché à une autre catégorie, journalisation de
  l'avertissement, résolution des libellés avec repli de langue, ordre d'affichage, rétro-
  compatibilité des manifestes sans `subcategory`.
- Les dépôts existants restent valides sans changement.
- Un dépôt tiers ne peut pas introduire de libellé ; en contrepartie, il dépend d'une
  publication de Forgearr pour qu'une nouvelle famille devienne disponible.

## Retour arrière

Ignorer le champ `subcategory` suffit : tous les plugins retombent sous leur catégorie, sans
migration (rien n'est stocké en base). Retirer le registre et le regroupement côté client rend
l'ancien affichage.
