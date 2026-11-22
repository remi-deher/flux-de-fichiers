# 0028 — Événements en temps réel

Statut : accepté par l'utilisateur (22 nov. 2026) ; phase A (catalogue, applications, dépôts, réglages de plugins).

## Contexte

L'interface ne voyait que ses propres changements : ceux d'un autre onglet, d'un autre compte, de l'IA ou du MCP
attendaient un rechargement. Le flux des tâches ignorait en silence un abonné en retard et, après un redémarrage
du central, les nouveaux numéros restaient sous l'ancien `Last-Event-ID`. En HTTP/1.1 un navigateur n'ouvre que
six connexions par hôte.

## Décision

1. **Un seul flux par page** : `GET /api/v1/events` porte les événements de tâches (noms et charge inchangés) et
   `changed`, de charge `{"topic": …}`. Aucune donnée métier : le client relit les routes, qui contrôlent les droits.
   `GET /api/v1/jobs/events` reste pour les clients existants, par le même code.
2. **Sujets** (`ChangeTopic`) : `catalog` (catalogue, librairie locale, blocs, variables exposées), `tools`,
   `repositories`, `settings` (réglages de plugins). Extensible ; un client ignore un sujet inconnu.
3. **Émission au plus bas niveau**, donc commune à l'API, au MCP, à l'IA et aux tâches de fond :
   - `CatalogCache::invalidate` émet `catalog` après avoir vidé le cache ; une lecture commencée avant ne le
     remplit plus ;
   - le cache d'outils du central (`AppState::tool_cache`) émet `tools` au début et à la fin d'une installation
     (échec et annulation compris) et à chaque purge ; le worker intégré en fait partie ;
   - les écritures SQL de dépôts et de réglages exigent un `ChangeNotifier` en paramètre. Les réglages émettent
     aussi `catalog`, dont l'état « à configurer » dépend.
4. **Après l'écriture seulement.** Une écriture de fichiers suivie d'une invalidation va à son terme même si le
   client coupe la connexion ; de même pour les écritures SQL signalées.
5. **Regroupement** : le premier changement d'un sujet ouvre une fenêtre de 300 ms ; l'événement part à sa fin.
   La fenêtre se ferme avant la publication : un changement plus tardif en ouvre une autre, rien n'est perdu.
6. **Reprise** : les numéros partent de l'horloge en microsecondes, ils croissent donc d'une instance à l'autre.
   Un `Last-Event-ID` hors de l'historique (256 événements), d'une autre instance ou illisible, ou un abonné en
   retard, reçoit `resync` : tout relire une fois. Un premier bloc sans donnée fixe la position du client, même
   s'il perd la connexion avant tout événement.
7. **Droits** : un sujet n'est remis qu'à qui peut lire ses routes (`ChangeTopic::viewers`), les tâches à qui porte
   `view_jobs` ; sans aucun droit de lecture, 403. L'accès est revérifié toutes les 30 s (session, compte, clé,
   portée) : un accès perdu ferme le flux. Une erreur passagère de la base ne le ferme pas.
8. **Worker** : pas de flux, la route répond 404 ; un navigateur ne réessaie pas après une telle réponse.
9. **Client** : un contexte `LiveEvents` par session, un signal par sujet, une seule connexion (les tâches y
   passent aussi). `live::load` relit une route quand ses sujets changent, garde l'ancien contenu pendant la
   relecture et lors d'une panne passagère, et ne redessine rien si le contenu est identique. Une écriture de la page
   fait relire ses sujets sans attendre le flux.
10. **Saisie en cours** : un formulaire non modifié de réglages suit le central ; modifié, il avertit « Modifié
    ailleurs » et propose « Recharger ». L'éditeur d'un script local avertit toujours, sans remplacer le texte lu.
    Les actions en cours et les aperçus vivent hors des fiches relues. Un tiroir dont l'élément actif est retiré par
    une relecture garde le focus ; à sa fermeture, la carte d'origine est retrouvée par son identité.
11. **Flux abandonné par le navigateur** (réponse autre que 200) : rouvert après 2 s, puis un délai doublé jusqu'à
    60 s, si la session est encore valide ; tout est relu à l'ouverture. De même si la première ouverture échoue
    ou dépasse 1 s : les premières lectures de la page ont pu manquer un changement. Jamais ouvert par un worker ni sans droit.

## Conséquences

- Un événement arrive environ 300 ms après l'écriture.
- L'éditeur de flux et les bibliothèques médias ne suivent pas encore le flux (phase B).
- Les caches d'outils des workers distants restent hors du flux.
