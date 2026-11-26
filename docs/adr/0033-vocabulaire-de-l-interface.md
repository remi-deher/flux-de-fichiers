# 0033 — Vocabulaire de l'interface

Statut : proposé. Termes validés par l'utilisateur le 26 nov. 2026 (« Catalogue » à la place de « Store »,
« pack » conservé, « Types de fichiers » à la place de « masque »). Lot 1 de la refonte de l'interface.
Complète l'ADR 0013 (central et worker).

## Contexte

Pour un même objet, l'interface mêlait plusieurs mots : « Flow », « Flows » et « Pipeline » pour un flux ;
« Librairie » et « Bibliothèque » pour un dossier de médias ; « Plugin » et « Extension » ; « Nœud » pour un
bloc ou pour un worker ; « Store » pour le catalogue ; « masque » pour le filtre de fichiers d'un flux.

## Décision

| Terme affiché | Définition | Remplace à l'écran |
|---|---|---|
| **Flux** (invariable) | Suite de blocs qui traite un fichier d'une bibliothèque | Flow, Flows, Pipeline |
| **Modèle de flux** | Flux réutilisable dont on part pour en créer un | « Modèle » seul |
| **Bibliothèque** | Dossier de médias surveillé, rattaché à des workers et à des flux | Librairie, Librairies |
| **Extension** | Ce qu'on installe depuis un dépôt pour ajouter des blocs, des outils ou des flux | Plugin, Plugins |
| **Bloc** | Une étape d'un flux (action, condition, transfert, script) | Nœud (dans un flux), élément |
| **Catalogue** | Onglet où l'on parcourt et installe les éléments des dépôts | Store |
| **Types de fichiers** | Filtre des fichiers qu'un flux ou une bibliothèque traite | masque |

Conservés : script, ressource, pack, worker, central (ADR 0013), installation, tâche. « Nœud » désignant une
machine est déjà retiré par l'ADR 0013 : on dit worker, ou installation quand le rôle est neutre.

- **Texte affiché seulement.** Routes d'API, champs JSON (`fileMask`…), noms de modules et de types gardent
  `flow`, `plugin` et `node`. Aucune migration.
- **Un module de constantes.** `client/src/ui/vocabulary.rs` porte les libellés partagés (navigation, titres,
  en-têtes) ; les autres textes sont écrits dans le vocabulaire ci-dessus.
- **Un test qui garde le vocabulaire.** `vocabulary::tests` parcourt les littéraux du client (hors modules de
  tests) et échoue sur tout texte affiché qui contient un mot banni (flow, librairie, plugin, nœud, store,
  masque, pipeline). Un identifiant (route, classe CSS, clé JSON) n'est pas du texte affiché : sans espace ni
  majuscule initiale, il passe.

## Hors périmètre de ce lot

- Les messages produits par le serveur (`src/flows/node_references.rs`, `src/flows/condition.rs`) et le libellé
  partagé « Nœuds précédents » (`shared/src/flows/node_references.rs`) disent encore « Nœud » : renommage à
  traiter avec leurs tests dans un lot de suivi.
- Les libellés de navigation et d'onglets de la refonte (Atelier, Flux › Modèles…) arrivent avec les lots 3 et 5.
