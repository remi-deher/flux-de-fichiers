# 0014 — Client web Leptos compilé en WASM, écran par écran

Statut : accepté par l'utilisateur (14 nov. 2026).

## Contexte

L'interface est générée une fois par Leptos en mode `ssr` (`frontend/dist/index.html`), mais tout
son comportement tient dans environ 4 500 lignes de JavaScript écrit à la main dans des chaînes
Rust (`frontend/src/client/`). Ce JavaScript lit le JSON de l'API sans aucun type : le renommage
de l'ADR 0013 a montré des champs (`nodeKind`) qu'aucun compilateur ni test ne voyait, et
l'éditeur de flux a longtemps détruit les flux à l'enregistrement sans que rien ne le signale.

## Décision

1. **Nouveau client Leptos en WASM** (crate `client/`, `flux-client`, Leptos `csr`), compilé
   pour `wasm32-unknown-unknown` puis lié par `wasm-bindgen --target web`.
2. **Servi sous `/app/`**, à côté de l'interface historique (`/`). Les routes sous `/app`
   retombent sur `app/index.html` ; la page est générée par `flux-frontend` avec les mêmes styles
   de thème.
3. **Migration écran par écran.** Un écran porté vit dans le client ; l'interface historique y
   renvoie. Quand tous les écrans sont portés, `/` sert le client et le JavaScript historique
   est supprimé. Aucun pont entre le JavaScript historique et le WASM.
4. **Contrat partagé** : les réponses lues par le client sont des types de `flux-shared`
   (`jobs`, `session`, `format`, `flows`, `libraries`, `workers`, `app`), utilisés aussi par le central ; les dérivations SQLx y sont
   derrière la fonctionnalité `sqlx`, que seul le central active.
5. **Connexion** : tant que le client n'a pas son écran d'accès, il réutilise la session cookie
   ouverte par l'interface historique (`/api/v1/auth/session` donne le jeton CSRF).
6. **Construction** : la cible WASM est déclarée dans `rust-toolchain.toml` ; l'image installe
   `wasm-bindgen-cli` à la version exacte du crate verrouillé (`WASM_BINDGEN_VERSION`). La CI
   vérifie le client en natif et pour `wasm32-unknown-unknown`, et teste `flux-shared` et
   `flux-client`.

## Écrans portés

- **Tâches** : liste en direct (SSE, progression), filtres par état, fiche, création depuis le
  dossier d'entrée et relance manuelle (ADR 0012).
- **Flux** : liste (version, composition), création, suppression, et éditeur :
  - graphe disposé en couches (plus long chemin depuis l'entrée), liaisons SVG par sortie ;
  - palette et formulaires générés depuis le **catalogue des blocs exécutables**
    (`flux_shared::flows::block_catalog`) : un champ décrit est un champ que le moteur lit ;
    une valeur enregistrée hors liste reste sélectionnable ;
  - sorties reliées une à une, avec refus des boucles, des sorties déjà prises et des entrées
    impossibles (`FlowGraph::connect`) ; retrait d'un bloc avec raccord d'un maillon simple ;
  - enregistrement du graphe complet (clés inconnues conservées), bloqué tant qu'un réglage
    obligatoire est vide ; un en-tête de webhook laissé vide garde sa valeur enregistrée.
- **Bibliothèques** : liste et état, création, modification (nom, flux, chemin et accès par
  worker), suppression.
- **Workers** : présence, worker intégré, retrait d'un worker hors ligne, commande de
  raccordement.

L'interface historique renvoie vers chacun de ces écrans (`NewInterfaceLink`), avec les chemins
de `flux_shared::app`.

Limites connues : les blocs d'un dépôt de plugins sans exécution native n'apparaissent pas dans
la palette (ils restent lus et conservés) ; pas de glisser-déposer ni de simulation ; quitter
l'éditeur ne prévient pas des modifications non enregistrées.

## Conséquences

- Pendant la transition, deux interfaces coexistent ; les tâches se créent et se relancent
  seulement dans la nouvelle.
- Ordre de portage restant : plugins, paramètres, accès et installation, vue d'ensemble, puis
  bascule de `/` et suppression du JavaScript historique.
- Le client ne s'exécute que dans un navigateur ; ses tests Rust portent sur le modèle (règles
  d'affichage, contrat), pas sur le rendu.

## Retour arrière

Retirer le crate `client`, son étape de construction et le renvoi `/app` ; l'interface
historique n'en dépend pas.
