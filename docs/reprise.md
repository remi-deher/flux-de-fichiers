# Reprise du chantier — état au 14 nov. 2026

Note de passage pour continuer avec un autre agent ou sur une autre machine. Les décisions
durables sont dans les ADR ; ce document dit où en est le code, ce qui n'est pas vérifié et ce
qui reste à faire. À mettre à jour (ou supprimer) quand l'état change.

## À lire d'abord

- `AGENTS.md` (règles permanentes), puis `docs/developpement.md` (où est le code, quoi vérifier).
- ADR récents : [0012](adr/0012-perimetre-perte-de-bail-et-versions-de-flows.md) (périmètre,
  perte de bail, versions de flux), [0013](adr/0013-vocabulaire-central-worker.md) (vocabulaire
  central / worker), [0014](adr/0014-client-leptos-wasm.md) (client Leptos WASM).
- Analyse du projet de référence `encoding-central` :
  [audit du 13 nov.](audits/2026-11-13-analyse-encoding-central.md).

## Règles de travail à respecter

- **Aucun test ni build Docker en local** (machine faible) : `cargo check`, `cargo clippy` et
  `cargo fmt` seulement ; les tests et l'image tournent en CI.
- Flux Git ([ADR 0002](adr/0002-flux-git.md)) : push direct sur `dev`, PR `dev → test` (CI
  complète, fusion automatique, image `:test-<sha12>`), PR `test → main` pour une release.
- **Jamais de ligne `Co-Authored-By` ni de mention d'agent** dans les commits et les PR.
- La machine locale n'a pas la cible `wasm32-unknown-unknown` (Rust d'Arch, sans rustup) :
  le client se vérifie en natif ici, en WASM seulement en CI et dans l'image.

## Décisions prises les 13 et 14 nov.

1. **Périmètre** : encodage par plugins (le remux seul servait aux essais) ; IA et MCP dans le
   périmètre ; installation neuve uniquement (pas de reprise d'une base `encoding-central`).
2. **Perte de bail** : échec `lease_expired`, jamais de remise en file automatique ; relance
   manuelle `POST /api/v1/jobs/{id}/retry`, 3 tentatives au plus.
3. **Flux versionnés** : table `flow_versions` immuable ; une tâche fige le flux par défaut et
   sa version à sa création.
4. **Vocabulaire** : central / worker / installation ; « nœud » réservé aux blocs d'un graphe.
5. **Interface** : client Leptos compilé en WASM (`client/`), servi à la racine ; l'ancienne
   page statique et ses ~4 500 lignes de JavaScript sont supprimées.
6. Fenêtre d'installation ouverte (sans jeton de setup) : choix assumé.

## Organisation du code

| Crate | Rôle |
|---|---|
| `.` (`flux-de-fichiers`) | Central et worker : API Axum, SQLite, moteur de flux, transferts |
| `shared/` (`flux-shared`) | **Contrat** lu par le central et le client : tâches, flux (graphe, catalogue des blocs exécutables), bibliothèques, workers, plugins, comptes, session, système, chemins |
| `client/` (`flux-client`) | Interface Leptos `csr` en WASM, un dossier par écran |
| `frontend/` (`flux-frontend`) | Page `dist/index.html` et thème CSS typé ; rien d'autre |

Écrans du client : accès et installation, vue d'ensemble, tâches, flux (éditeur), bibliothèques,
workers, plugins, paramètres. Sur un worker : vue d'ensemble, tâches et plugins (lecture),
paramètres.

## Non vérifié (à faire en premier)

- Le client compile en WASM (CI et image `test-34f365e9af28`) et affiche son écran de connexion
  en production ; **aucun écran connecté n'a encore été ouvert**.
- Aucun parcours de bout en bout : connexion, création et relance d'une tâche, édition et
  enregistrement d'un flux, bibliothèques, plugins, comptes.
- Les tests ajoutés depuis le 13 nov. n'ont tourné qu'en CI (verte sur la PR #102).

## Limites connues

**Interface**

- Gabarit d'origine repris dans le client (14 nov., après retour de l'utilisateur) : barre
  latérale repliable par groupes, barre du haut avec recherche contextuelle (filtre les lignes de
  l'écran) et workers en ligne, porte d'accès `auth-gate`, sous-navigation par écran, registres
  et tiroirs d'inspection pour les fiches et formulaires (`client/src/ui/`, `client/src/shell.rs`).
  Non repris : widgets personnalisables de la vue d'ensemble, feuille de filtres de la barre du
  haut (remplacée par les onglets), onglets Réseau et Stockage, télémétrie matérielle, page Mises
  à jour, import de flux depuis le catalogue (route inexistante), simulation animée.
- Éditeur de flux : pas de glisser-déposer, pas d'avertissement en quittant avec des
  modifications, pas de consultation ni de restauration des versions ; les blocs des dépôts de
  plugins n'apparaissent pas dans la palette ; le bloc vidéo n'a pas de réglages d'encodage.
- Tâches : pas de choix du worker à la création ; 200 tâches au plus, sans pagination.
- Session expirée en cours d'usage : message d'erreur, retour à la connexion seulement en
  rechargeant la page.
- Pas d'écran « mot de passe oublié » (`flux-de-fichiers reset-password`) ; rotation de clé en
  ligne de commande (`rotate-secret`) ; pas de changement de mot de passe sur un worker.
- Accessibilité non auditée ; sans JavaScript ni WebAssembly, aucune interface.

**Moteur et produit**

- Workers distants : le flux figé voyage avec le bail (`Claim.frozen_flow`) et s'exécute via
  `FlowEngine` comme le worker intégré ; les en-têtes webhook sont déchiffrés à la réclamation
  et le contexte *arr* est joint (`Claim.arr`). Reste : pas de progression SSE depuis un worker
  distant ; encodage par plugins toujours hors périmètre d'exécution.
- Encodage par plugins non construit ; blocs des dépôts de plugins non exécutables (traversés).
- IA et MCP non commencés.
- Baux sans numéro d'époque (*fencing*) : un ancien worker peut encore répondre tard.
- Requêtes SQL non vérifiées à la compilation (`query!` non adopté, voir l'audit).
- `frontend/dist/index.html` est versionné : le régénérer (`cargo run -p flux-frontend`) après
  toute modification du thème ou de la page.

## Suite proposée

1. Se connecter à l'essai et parcourir chaque écran ; corriger ce que le WASM révèle.
2. Encodage par plugins : blocs des dépôts dans la palette et exécution.
3. Progression SSE / télémétrie fine depuis les workers distants pendant l'exécution d'un flux.
4. Numéro d'époque des baux.
5. Écrans manquants : versions des flux, recherche, navigation mobile.
6. IA et MCP (ADR à écrire).

## Production (NAS)

**Déployé le 14 nov. 2026** : `mrcryllix/flux-de-fichiers:test-34f365e9af28` sur les deux hôtes
(CI verte, client WASM compilé). Vérifié : santé, page et WASM servis, migrations 12 et 13,
worker enregistré qui réclame des tâches, écran de connexion du client affiché sur les deux.
Non vérifié : les parcours connectés (identifiants de l'utilisateur). Sauvegardes et retour
arrière : journal de Rashin du 14 nov. (`compose-*.avant-0014.json`,
`arr-config.bak-test-28a81f724ea4-avant-0014-froid`, `frontend/index.html.avant-0014`).

Essai en conditions réelles : central `flux-essai` sur `truenas_admin@192.168.1.50`
(HTTP `:18480`), worker `flux-essai-worker` (`artemis-essai`) sur `truenas_admin@192.168.1.51`
(HTTP `:18481`). Applications TrueNAS mises à jour par
`midclt call -j app.update` avec `custom_compose_config` ; procédure détaillée et retour arrière
dans [deploiement-essai-0011.md](deploiement-essai-0011.md). Points propres à ce lot :

- le worker lit `CENTRAL_URL` (et non plus `COORDINATOR_URL`, ADR 0013) ;
- le central sert l'interface depuis le volume `…/essai25/frontend` (lecture seule) : y recopier
  `index.html` **et** le dossier `pkg/` de l'image ;
- migrations 0012 (versions de flux) et 0013 (`library_workers`) appliquées au démarrage ;
  sauvegarder `arr-config` avant.
