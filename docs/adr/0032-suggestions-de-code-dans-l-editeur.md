# 0032 — Suggestions de code dans l'éditeur de script

Statut : proposé ; phase (a) écrite le 24 nov. 2026 (en attente de la CI), phases (b) et (c) à venir. Demande de l'utilisateur le 24 nov. 2026 : des suggestions « comme dans un vrai IDE », par intelligence de langage (LSP). À cadrer avant tout code ; complète l'ADR 0030.

## Contexte

L'éditeur (ADR 0030) colore le code mais ne suggère rien. Il accepte sept interpréteurs (`ScriptInterpreter` : sh, bash, python3, node, deno, bun, pwsh). L'image du central est unique et n'embarque aucun outil externe (ADR 0012 : l'encodage passe par des plugins). Un serveur de langage est un programme par langage, à installer, à faire vivre et à isoler.

## Questions à trancher

1. **Où tourne le serveur de langage ?** Central (image alourdie, contraire à l'esprit « conteneur unique léger »), ou plugin installable, ou worker. Recommandation : plugin « langage » installable, jamais embarqué dans l'image, comme l'encodage.
2. **Quels langages en premier ?** Les seuls serveurs matures et légers : `bash-language-server` (bash/sh), `pyright` ou `pylsp` (Python), `typescript-language-server` (node/deno/bun), PowerShell Editor Services (pwsh, lourd). Recommandation : bash et Python d'abord.
3. **Transport.** LSP parle JSON-RPC sur stdio. Le central relaierait une session par éditeur ouvert (WebSocket ou SSE + POST), avec délai d'inactivité, une session par script, droit « gérer les plugins ».
4. **Contrat Forgearr.** Le serveur de langage ignore le contrat `stdin`/`stdout` JSON ; les champs (`workingFile`, `variables`, `nextPort`…) et les variables déclarées seraient ajoutés par une liste de suggestions propre à Forgearr, fusionnée aux réponses du LSP.
5. **Interface.** Liste de suggestions sous le curseur (flèches, Tab/Entrée, Échap), diagnostics soulignés, info-bulle : à ajouter à `CodeEditor` sans changer sa grille `textarea`/`pre`.

## Décision (provisoire)

Phaser : (a) liste de suggestions côté client sans serveur de langage (mots-clés, mots du script, contrat Forgearr, variables déclarées), qui sert aussi de repli ; (b) session LSP relayée par le central pour bash puis Python ; (c) diagnostics et info-bulles. Aucune bibliothèque JavaScript d'éditeur.

## Phase (a) écrite

`client/src/libraries/suggestions.rs` (calcul pur, testé) et la liste de `code_editor.rs`. Après deux caractères, la liste propose, dans l'ordre : variables déclarées du script, champs du contrat JSON, mots-clés de l'interpréteur, mots déjà présents dans le script (huit au plus). Flèches pour choisir, Tab ou Entrée pour accepter, Échap pour fermer ; un clic accepte aussi. La liste se place sous le curseur par la ligne et la colonne (police fixe), sans mesure du DOM.

## Conséquences

- La stack ne change pas ; les serveurs de langage vivent hors de l'image.
- Sans serveur installé pour un langage, l'éditeur garde la liste (a).
- Tests : relais de session (droits, expiration, arrêt du processus), fusion des suggestions, navigation au clavier.
