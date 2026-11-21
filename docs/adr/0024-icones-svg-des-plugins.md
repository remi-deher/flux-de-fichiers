# 0024 — Icônes SVG des plugins

Statut : accepté par l'utilisateur (21 nov. 2026) ; implémenté.

## Contexte

Toutes les cartes du catalogue affichent la même icône générique, choisie par type de pièce et
non par plugin. Le schéma du manifeste déclare un champ `icon` (chemin d'une icône du paquet)
que rien ne lisait : la synchronisation (ADR 0010) ne copie que du JSON et aucune route ne sert
d'autre fichier. Une icône vient d'un dépôt de plugins, donc potentiellement tiers : un SVG peut
embarquer du script, des styles ou des références externes.

## Décisions

1. **Le dépôt de plugins porte l'icône.** Le manifeste déclare `icon` : un chemin relatif au
   dossier du manifeste, terminé par `.svg` (par exemple `icon.svg`).
2. **Liste blanche stricte.** Un SVG n'est retenu que s'il pèse 16 Kio au plus et ne contient
   qu'un élément racine `svg` (espace de noms SVG exact), des éléments de dessin (`g`, `path`,
   `circle`, `ellipse`, `rect`, `line`, `polyline`, `polygon`) et des attributs de géométrie, de
   remplissage et de trait. Sont refusés : tout texte, commentaire, déclaration ou instruction,
   `style`, `href`, gestionnaires d'événements, `use`, `image`, `foreignObject`, `script`, et toute
   valeur d'attribut contenant `:`, `&`, `<`, `>` ou `\`. Le validateur est le même pour le dépôt
   officiel (`check_catalogue.py`) et pour Forgearr (`src/plugins/icon.rs`).
3. **Validation à chaque étape.** Forgearr valide l'icône à la synchronisation (une icône refusée
   n'est pas copiée et est signalée dans `SyncReport.skipped`), puis de nouveau avant chaque
   service. En mode release immuable, l'empreinte SHA-256 de l'icône est exigée et vérifiée
   comme celle des autres fichiers annexes ; `build_index.py` l'ajoute à `filesSha256`.
4. **Route de service.** `GET /api/v1/plugins/{id}/icon` (droit de voir les plugins) renvoie
   l'icône en `image/svg+xml` avec `Content-Security-Policy: default-src 'none'; sandbox`,
   `X-Content-Type-Options: nosniff` et `Cache-Control: private, max-age=300`. Le chemin est
   résolu dans le dossier du manifeste, sans `..` ni chemin absolu. Plugin inconnu, sans icône ou
   à l'icône refusée : 404.
5. **Contrat.** `CatalogEntry.hasIcon` (absent s'il est faux) indique qu'une icône est déclarée
   et présente dans la copie locale ; le client n'appelle la route que dans ce cas.
6. **Rendu.** Le client affiche l'icône comme masque d'image CSS (`.plugin-icon`,
   `mask` + `background-color: currentColor`) : elle prend la couleur du texte du thème, clair
   comme sombre, et n'est jamais chargée comme document. Sans icône, la carte garde l'icône
   générique actuelle.

## Conséquences

- Pas de nouveau format de fichier côté synchronisation : l'icône est du texte UTF-8 de 16 Kio
  au plus, copié comme le reste dans le dossier du dépôt.
- Une icône ne peut pas être colorée par le dépôt : elle hérite de la couleur du thème. Les
  dépôts dessinent en `currentColor` ou sans couleur.
- Tests : validateur (cas valides et hostiles), résolution du chemin, collecte à la
  synchronisation (icône refusée, introuvable, empreinte), route (en-têtes, 404, authentification)
  et indicateur du catalogue.

## Retour arrière

Ne plus lire `icon` ramène l'icône générique, sans migration : rien n'est stocké en base.
