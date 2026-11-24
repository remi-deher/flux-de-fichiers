# 0030 — Éditeur de script en pleine page

Statut : proposé. Forme (page plein écran) choisie par l'utilisateur le 23 nov. 2026. Lot 1 écrit le 23 nov. 2026
et publié en 0.84.0 ; lot 2 écrit le 24 nov. 2026 (en attente de la CI) ; lot 3 à venir. Modifie l'ADR 0014 (écrans du client) et complète l'ADR 0025 (édition des variables et dépendances).

## Contexte

Un script local s'écrit aujourd'hui dans un tiroir (`Drawer`, `client/src/libraries/scripts.rs`) : un
`<textarea rows="12">` sans numéros de ligne ni coloration, partagé avec tout le formulaire (nom, interpréteur,
description, catégorie, étiquettes, variables, dépendances). La place manque pour écrire du code, et le
formulaire devient long. Le moteur ne change pas : le contrat `stdin`/`stdout` JSON (ADR 0017 et 0025) est
inchangé, les gabarits par interpréteur existent déjà (`starter_script_code`).

## Décision

1. **Deux routes.** `/app/libraries/scripts/new` et `/app/libraries/scripts/<id>` (`Route::NewScript`,
   `Route::ScriptEditor(id)`), rattachées à l'entrée « Librairies » de la navigation, comme `FlowEditor` l'est à
   « Flux ». Le tiroir de création est retiré ; la carte d'un script ouvre la page.
2. **Mise en page.** Zone de code à gauche (numéros de ligne, police fixe, tabulation qui insère deux espaces — Échap libère la touche Tab une fois, pour quitter la zone au clavier ;
   coloration en direct) ; panneau à droite : nom et interpréteur, puis onglets « Informations », « Variables »
   et « Dépendances » (les éditeurs existants, déplacés tels quels). Barre fixe : Enregistrer, Fermer,
   indicateur « modifié ».
3. **Coloration sans bibliothèque JavaScript.** Un `<textarea>` transparent superposé à un `<pre>` coloré, défilant
   ensemble. Les jetons viennent du central : `POST /api/v1/highlight` (`code`, `extension`) renvoie
   `{lines: Vec<Vec<CodeToken>>}` via `highlight_code` (syntect, côté serveur, hors du fil asynchrone). Appel
   différé de 350 ms après la dernière frappe ; les jetons ne s'affichent que s'ils ont été calculés pour le
   texte exact présent, sinon le texte brut reste affiché (la couche colorée ne se décale jamais). Corps limité
   à 1 Mio (limite HTTP des routes de scripts). Droit requis : gérer les plugins, comme l'enregistrement. La grammaire PowerShell et le repli en texte brut sont ceux de `highlight_code`.
4. **Quitter sans perdre.** La page déclare ses modifications au `Router` (`unsaved`) : la garde
   `needs_leave_confirmation` existante demande confirmation. La détection de modification concurrente du script
   ouvert (`opened_script_warning`) est conservée.
5. **Stack inchangée.** Pas de Monaco ni de CodeMirror : auto-complétion, repli de blocs et recherche
   sont hors périmètre.

## Phasage

- **Lot 1 — page et routes.** `Route`, rattachement à Librairies, page avec les champs actuels déplacés, garde de
  sortie, retrait du tiroir de création et d'édition.
- **Lot 2 — éditeur de code.** Numéros de ligne, tabulation, superposition `textarea`/`pre`, `POST /api/v1/highlight`.
- **Lot 3 — essai (optionnel, ADR à part).** Bouton « Tester » : exécuter le script sur un exemple et montrer
  stdin, stdout, stderr et la réponse interprétée.

## Conséquences

- Tests : analyse et rendu des deux routes (`client/src/route.rs`), garde de sortie, route de coloration (droits,
  taille, extension inconnue en texte brut), régression sur l'enregistrement d'un script (`tests/local_items_api.rs`).
- Le défilement synchronisé et le décalage entre `textarea` et `pre` (mêmes police, interligne, remplissage) sont
  le point délicat du lot 2 ; ils se vérifient dans le navigateur, hors CI.
- Un script d'un dépôt (lecture seule) garde son aperçu actuel (`script_preview.rs`) ; seule la copie locale
  s'ouvre dans l'éditeur.
