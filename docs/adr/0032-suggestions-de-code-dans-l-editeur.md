# 0032 — Suggestions de code dans l'éditeur de script

Statut : proposé ; phase (a) livrée en 0.87.0 ; phases (b) et (c) cadrées le 26 nov. 2026 avec l'utilisateur ; lots b0, b1, b2 et c1 écrits, la suite à venir. Demande de l'utilisateur le 24 nov. 2026 : des suggestions « comme dans un vrai IDE », par intelligence de langage (LSP). Complète l'ADR 0030.

## Contexte

L'éditeur (ADR 0030) colore le code mais ne suggère rien. Il accepte sept interpréteurs (`ScriptInterpreter` : sh, bash, python3, node, deno, bun, pwsh). L'image du central est unique et n'embarque aucun outil externe (ADR 0012 : l'encodage passe par des plugins). Un serveur de langage est un programme par langage, à installer, à faire vivre et à isoler.

## Questions à trancher

1. **Où tourne le serveur de langage ?** Central (image alourdie, contraire à l'esprit « conteneur unique léger »), ou plugin installable, ou worker. Recommandation : plugin « langage » installable, jamais embarqué dans l'image, comme l'encodage.
2. **Quels langages en premier ?** Les seuls serveurs matures et légers : `bash-language-server` (bash/sh), `pyright` ou `pylsp` (Python), `typescript-language-server` (node/deno/bun), PowerShell Editor Services (pwsh, lourd). Recommandation : bash et Python d'abord.
3. **Transport.** LSP parle JSON-RPC sur stdio. Le central relaierait une session par éditeur ouvert (WebSocket ou SSE + POST), avec délai d'inactivité, une session par script, droit « gérer les plugins ».
4. **Contrat Forgearr.** Le serveur de langage ignore le contrat `stdin`/`stdout` JSON ; les champs (`workingFile`, `variables`, `nextPort`…) et les variables déclarées seraient ajoutés par une liste de suggestions propre à Forgearr, fusionnée aux réponses du LSP.
5. **Interface.** Liste de suggestions sous le curseur (flèches, Tab/Entrée, Échap), diagnostics soulignés, info-bulle : à ajouter à `CodeEditor` sans changer sa grille `textarea`/`pre`.

## Décision

Phaser : (a) liste de suggestions côté client sans serveur de langage, qui sert aussi de repli ; (b) session LSP relayée par le central pour bash puis Python ; (c) diagnostics et info-bulles. Aucune bibliothèque JavaScript d'éditeur. Les choix ci-dessous ont été arrêtés par l'utilisateur le 26 nov. 2026.

1. **Où.** Les serveurs de langage tournent sur le central uniquement. Un serveur sur un worker distant (session bavarde par le courtier à attente longue) est reporté.
2. **Livraison.** Une application de catalogue `langages` (capacité `language-server`), autonome et jamais embarquée dans l'image : `ty` (Python, binaire natif, `ty server`), `shellcheck` (binaire statique, diagnostics bash et sh, `--shell=sh` pour sh) et `bash-language-server` avec son propre Node.js embarqué (environ 100 Mo). Aucune dépendance à `interpreteurs`. Le central compose la commande depuis le cache d'outils (`prepare_command`, environnement vidé). Un essai de `ty` sur des scripts réels ouvre le lot b0 ; si la complétion ou les infobulles déçoivent, `pyright` le remplace sans changer le reste.
3. **Canal.** Un WebSocket `GET /api/v1/language/session?interpreter=…`, à messages Forgearr typés (énumération Serde dans `shared/`), jamais du JSON-RPC brut : le navigateur ne peut envoyer aucune commande LSP arbitraire. Client vers central : `open`, `change`, `complete`, `hover`, `close`. Central vers client : `ready`, `diagnostics` (poussés dès que le serveur les publie), `completion`, `hover`, `unavailable` (raison : serveur absent, plafond atteint, serveur arrêté). Droit « gérer les plugins », comme l'éditeur ; session web, droit et origine vérifiés à l'ouverture, accès revérifié périodiquement comme le flux d'événements (ADR 0028). Message de plus de 1 Mio : canal fermé. Il faut activer la fonction `ws` d'Axum et celle de `web-sys` ; le reverse proxy externe doit laisser passer la mise à niveau WebSocket (ADR 0005), sinon l'éditeur reste sur la liste (a).
4. **Sessions.** Un processus par éditeur ouvert, lié au compte, plafond global réglable (4 par défaut) ; au-delà, `unavailable`. Dossier de travail jetable et vide, document en mémoire seulement (`didOpen`), URI `file:///forgearr/<session>/script.<ext>`. Le lancement passe par l'exécution confinée de l'[ADR 0034](0034-execution-confinee.md) (profil : lecture du dossier d'outil, écriture dans le dossier jetable, pas de réseau). Arrêt après 10 minutes sans message, à la fermeture du canal et au retrait de l'application (`shutdown`, `exit`, puis `kill_on_drop`). Méthodes envoyées : `initialize`, `didOpen`, `didChange` (texte entier), `completion`, `completionItem/resolve`, `hover`, `shutdown` ; jamais `workspace/executeCommand`. Réponses bornées : 64 Kio par message du serveur, 200 suggestions, 2 s par requête.
5. **Fusion.** Variables déclarées et champs du contrat Forgearr d'abord, puis le serveur, puis les mots du script ; doublons retirés par libellé. Sans réponse en 800 ms, la liste (a) s'affiche seule ; le canal se rouvre avec un délai croissant.
6. **Imports Python tiers.** L'avertissement « import introuvable » est masqué : les paquets vivent sur les workers, pas sur le central. La bibliothèque standard reste complétée.
7. **ADR 0009.** Un serveur de langage est un outil (application) lancé par l'éditeur, pas un bloc de flux : l'exception est inscrite dans l'ADR 0009.

## Phasage de (b) et (c)

- **b0 — application `langages` (dépôt de plugins).** Essai de `ty` ; manifeste autonome ; `ty` et `shellcheck` en archives amont épinglées par SHA-256 ; `bash-language-server` par un nouveau mode de construction `npm-packages` (verrou figé, sans script d'installation, empreintes d'intégrité npm, Node embarqué) ; README et licences.
- **b1 — canal et sessions (application).** Types partagés, codec JSON-RPC (`Content-Length`), gestionnaire de sessions, route WebSocket, documentation du reverse proxy.
- **b2 — suggestions du serveur dans l'éditeur.** Ouverture et fermeture du canal par la page, fusion pure avec la liste (a), caractères déclencheurs (`.` en Python, `$` en bash), repli.
- **c1 — diagnostics.** Soulignement dans la couche `pre` (jetons découpés aux bornes), marque dans la gouttière, compteur dans la barre.
- **c2 — info-bulles.** Au survol après 500 ms et au clavier (Ctrl+K I), Markdown rendu en texte sûr, sans HTML brut.
- **c3 — diagnostics Forgearr.** Règles pures, utiles même sans serveur installé : `nextPort` inconnu du bloc, variable lue mais non déclarée, sortie JSON absente. Après c1.

b0 et b1 avancent en parallèle (b1 se teste avec un faux serveur sur `tokio::io::duplex`).

## Lot b1 écrit

- **Messages.** `shared/src/language.rs` : `ClientMessage` (`open`, `change`, `complete`, `hover`, `close`) et `ServerMessage` (`ready`, `diagnostics`, `completion`, `hover`, `unavailable`), étiquetés par `type`, en `camelCase`. Le serveur de langage d'un interpréteur est décidé par `LanguageServer::for_interpreter`.
- **Route.** `GET /api/v1/language/session?interpreter=…`, droit `manage_plugins`, cookie de session (le navigateur ne pose pas d'en-tête), `Origin` contrôlé, messages de la page limités à 1 Mio, droits revérifiés toutes les 30 s. Quand aucun serveur ne répond, la page reçoit `unavailable` (`interpreter-unsupported`, `server-missing`, `limit-reached`, `server-stopped`) puis la fermeture, et garde sa liste (a).
- **Session.** `src/language/session.rs` relaie sur des flux génériques : poignée de main de 15 s, requête bornée à 2 s, 64 Kio par message du serveur, 200 suggestions, arrêt après 10 min sans message de la page. Les diagnostics sont poussés dès que le serveur les publie, ou tirés (`textDocument/diagnostic`) quand il le propose. Les imports Python tiers non résolus sont masqués.
- **Lancement.** `src/language/launch.rs` : `sandbox-exec` (ADR 0034) avec lecture du dossier de l'application `langages`, écriture dans `<données>/language/<session>` (supprimé à la fin, porte `HOME` et les caches), aucun réseau, environnement vidé. La session retient `tools_in_use` : la purge des outils attend sa fin. Plafond de 4 sessions (`LANGUAGE_SESSIONS_MAX`, 32 au plus).
- **Reverse proxy.** Le canal est un WebSocket : le proxy doit transmettre `Upgrade` et `Connection`, et ne pas couper une connexion inactive trop vite (la page ne parle que lorsque l'utilisateur tape ; nginx : `proxy_http_version 1.1`, `proxy_set_header Upgrade $http_upgrade`, `proxy_set_header Connection "upgrade"`, `proxy_read_timeout` d'au moins 10 min).

## Lot b2 écrit

- **Canal.** `client/src/libraries/language_channel.rs` : un WebSocket par éditeur, ouvert avec lui (si l'interpréteur a un serveur, `LanguageServer::for_interpreter`) et fermé avec lui (`close` arrête le serveur). Le texte entier est envoyé à l'ouverture, puis seulement quand il a changé au moment d'une demande.
- **Demandes.** Une demande part après deux caractères, ou aussitôt après un caractère déclencheur : `.` en Python, `$` en shell. Une réponse arrivée après une frappe plus récente est ignorée.
- **Fusion.** `language_logic::merge` : les suggestions du serveur d'abord, dans son ordre et filtrées par le mot tapé, puis ce que la liste locale (a) ajoute, sans doublon, huit au plus. La liste locale s'affiche aussitôt et se complète à la réponse du serveur.
- **Repli.** Un interpréteur sans serveur ou une application `langages` absente laissent la liste locale seule. Un canal tombé, un plafond atteint ou un serveur arrêté pour inactivité se rouvrent à la demande suivante, après une attente qui double à chaque échec (celle des flux d'événements : 2 s, plafond 60 s). Aucun minuteur ne tourne en arrière-plan.
- Les diagnostics et les info-bulles reçus sont ignorés jusqu'aux lots c1 et c2.

## Lot c1 écrit

- **Texte donné au serveur.** Après 400 ms sans frappe, l'éditeur envoie son texte au serveur de langage (`LanguageChannel::follow`), qui publie ses diagnostics (poussés, ou tirés à chaque changement pour `ty`). Les suggestions envoient aussi le texte avant de demander.
- **Soulignement.** `client/src/libraries/diagnostic_marks.rs` coupe les jetons colorés aux bornes de chaque anomalie (colonnes UTF-16) sans jamais changer le texte : la couche `pre` garde la largeur de la saisie. Ondulé rouge pour une erreur, ondulé orange pour un avertissement, pointillé pour une information ou une suggestion ; la plus grave l'emporte quand les plages se recouvrent.
- **Gouttière et résumé.** Le numéro de la ligne prend la couleur de l'anomalie la plus grave (son message en infobulle) ; une ligne sous la zone résume (« 2 erreurs · 1 avertissement »).
- **Texte modifié depuis le calcul.** Les anomalies sont rattachées au texte pour lequel le serveur les a calculées : à la frappe, celles des lignes inchangées en haut et en bas restent (les dernières suivent le décalage de lignes), celles des lignes touchées disparaissent jusqu'à la publication suivante. Elles disparaissent aussi quand le canal se ferme.

## Phase (a) écrite

`client/src/libraries/suggestions.rs` (calcul pur, testé) et la liste de `code_editor.rs`. Après deux caractères, la liste propose, dans l'ordre : variables déclarées du script, champs du contrat JSON, mots-clés de l'interpréteur, mots déjà présents dans le script (huit au plus). Flèches pour choisir, Tab ou Entrée pour accepter, Échap pour fermer ; un clic accepte aussi. La liste se place sous le curseur par la ligne et la colonne (police fixe), sans mesure du DOM.

## Conséquences

- La stack ne change pas ; les serveurs de langage vivent hors de l'image.
- Sans serveur installé pour un langage, l'éditeur garde la liste (a).
- Tests : session écrite sur `AsyncRead`/`AsyncWrite` génériques (droits, expiration, plafond, arrêt du processus, message trop gros), fusion des suggestions, découpage des jetons aux bornes des diagnostics (multi-lignes, UTF-16), règles de c3, navigation au clavier.
- Limite connue : une tabulation collée ou un caractère large décale soulignements et info-bulles, comme la liste (a) aujourd'hui.
- Hors périmètre : node, deno, bun (`typescript-language-server`) et pwsh (très lourd) restent sur la liste (a) ; le même mécanisme les accueillera.
