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
   (`jobs`, `session`, `format`), utilisés aussi par le central ; les dérivations SQLx y sont
   derrière la fonctionnalité `sqlx`, que seul le central active.
5. **Connexion** : tant que le client n'a pas son écran d'accès, il réutilise la session cookie
   ouverte par l'interface historique (`/api/v1/auth/session` donne le jeton CSRF).
6. **Construction** : la cible WASM est déclarée dans `rust-toolchain.toml` ; l'image installe
   `wasm-bindgen-cli` à la version exacte du crate verrouillé (`WASM_BINDGEN_VERSION`). La CI
   vérifie le client en natif et pour `wasm32-unknown-unknown`, et teste `flux-shared` et
   `flux-client`.

## Premier écran porté

Tâches : liste en direct (SSE, progression), filtres par état, fiche, création d'une tâche
depuis le dossier d'entrée et relance manuelle (ADR 0012), selon les droits de l'appelant.

## Conséquences

- Pendant la transition, deux interfaces coexistent ; les tâches se créent et se relancent
  seulement dans la nouvelle.
- Ordre de portage proposé : éditeur de flux (graphe, formulaires générés depuis les schémas des
  blocs, versions), bibliothèques, workers, plugins, paramètres, accès et installation, puis
  bascule de `/` et suppression du JavaScript historique.
- Le client ne s'exécute que dans un navigateur ; ses tests Rust portent sur le modèle (règles
  d'affichage, contrat), pas sur le rendu.

## Retour arrière

Retirer le crate `client`, son étape de construction et le renvoi `/app` ; l'interface
historique n'en dépend pas.
