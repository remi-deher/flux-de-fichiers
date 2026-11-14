# 0015 — IA et MCP

Statut : proposé (14 nov. 2026), cadré avec l'utilisateur ; les questions ouvertes ci-dessous
restent à trancher avant tout code.

## Contexte

L'ADR 0012 place l'IA et le MCP dans le périmètre sans en fixer le contenu. Réponses de
l'utilisateur du 14 nov. 2026 :

- l'IA doit **suggérer, analyser et créer**, sur les trois objets du produit : **bibliothèques,
  flux et plugins** ;
- le MCP **expose le central à des clients externes** (le central est serveur MCP ; il ne
  consomme pas de serveurs MCP tiers).

## Décision

1. **Le central est un serveur MCP.** Les outils exposés reprennent l'API existante, sans
   seconde logique métier : un outil MCP appelle la même couche que la route HTTP
   correspondante, avec les mêmes types de `flux-shared`.
2. **Trois familles d'outils**, par objet (bibliothèques, flux, plugins) :
   - *analyser* : lecture seule (état d'une bibliothèque, validité et historique d'un flux,
     réglages d'un plugin, échecs de tâches) ;
   - *suggérer* : renvoie une proposition (un diff de flux, un réglage, un plugin à activer)
     **sans l'appliquer** ;
   - *créer* : crée ou modifie l'objet, soumis aux règles de 4 et 5.
3. **Authentification par clé d'API existante** (admin / worker, ADR 0011). Une nouvelle clé de
   rôle `mcp`, au périmètre limité, est préférable à la réutilisation d'une clé admin. Les
   outils d'analyse et de suggestion sont ouverts à ce rôle ; ceux de création demandent un
   droit explicite.
4. **Les créations passent par les versions de flux** (ADR 0012) : un flux créé ou modifié par
   l'IA devient une nouvelle version immuable, attribuée à son auteur (clé `mcp`), donc
   restaurable. Les bibliothèques et les plugins n'ont pas cette garantie : leur création par
   l'IA reste **désactivée par défaut** (réglage de l'installation).
5. **Aucun secret ne sort par un outil** : en-têtes de flux, clés d'API, jetons *arr* et webhooks
   restent masqués, comme dans l'interface.
6. **Journalisation** locale de chaque appel d'outil (clé, outil, objet, résultat), sans le
   contenu des secrets.
7. **Transport** : HTTP sur le port unique existant (route dédiée), pas de second port.
8. **Le central ne contient pas de modèle.** « Suggérer » et « créer » côté serveur supposent un
   fournisseur d'IA (voir questions) ; tant qu'il n'est pas choisi, le MCP n'expose que les
   outils déterministes (analyser, appliquer une proposition fournie par le client).

## Questions ouvertes

- **Qui génère les suggestions** : le client MCP externe (le central ne fait qu'exposer des
  outils, aucun modèle embarqué) ou un fournisseur configuré dans le central (clé d'API,
  modèle local) ? Le choix change le périmètre : le premier est bien plus simple et ne fait
  voyager aucune donnée hors de l'installation.
- Faut-il une **validation humaine** dans l'interface avant d'appliquer une suggestion ou une
  création de bibliothèque ou de plugin ?
- Créer un **plugin** : générer un manifeste et des blocs (JSON, dépôt `flux-de-fichiers-plugins`)
  seulement, ou aussi du code exécutable ? Le second est exclu tant que les plugins ne sont pas
  isolés.
- Limites de débit et de coût par clé.

## Conséquences

- Nouvelle crate ou module `mcp` du central, un fichier par famille d'outils, testé comme les
  routes (tests natifs).
- Nouveau rôle de clé et ses tests d'autorisation dans `flux-shared`.
- Aucun changement de la stack (ADR 0001) ; la dépendance MCP (SDK Rust) est à choisir à
  l'implémentation, après vérification de sa maintenance.
