# Reprise du chantier — état au 14 nov. 2026 (après #105/#106/#108)

Note de passage pour continuer avec un autre agent ou sur une autre machine. Les décisions
durables sont dans les ADR ; ce document dit ce qui est **livré**, ce qui n'est pas vérifié et
ce qui **reste à faire**. À mettre à jour (ou supprimer) quand l'état change.

## À lire d'abord

- `AGENTS.md` (règles permanentes), puis `docs/developpement.md` (où est le code, quoi vérifier).
- ADR récents : [0012](adr/0012-perimetre-perte-de-bail-et-versions-de-flows.md) (périmètre,
  perte de bail, versions de flux), [0013](adr/0013-vocabulaire-central-worker.md) (vocabulaire
  central / worker), [0014](adr/0014-client-leptos-wasm.md) (client Leptos WASM).
- Analyse du projet de référence `encoding-central` :
  [audit du 13 nov.](audits/2026-11-13-analyse-encoding-central.md).

## Règles de travail à respecter

- **Aucun test ni build Docker en local** (machine faible) : `cargo check`, `cargo clippy` et
  `cargo fmt` seulement ; les tests et l'image tournent en CI (`dev` n'a pas de CI ; la CI
  complète est sur les PR `dev → test`, ADR 0002).
- Flux Git ([ADR 0002](adr/0002-flux-git.md)) : push / PR feature → `dev` (merge commit), puis
  PR `dev → test` (CI, fusion auto, image `:test-<sha12>`), PR `test → main` pour une release.
- **Jamais de ligne `Co-Authored-By` ni de mention d'agent** dans les commits et les PR.
- La machine locale n'a pas la cible `wasm32-unknown-unknown` (Rust d'Arch, sans rustup) :
  le client se vérifie en natif ici, en WASM seulement en CI et dans l'image.
- Accès NAS uniquement depuis DELL-LAPTOP : central `truenas_admin@192.168.1.50:18480`,
  worker `.51:18481`.

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

## Livré (fusionné sur `dev`)

| Sujet | PR / dépôt | Contenu |
|---|---|---|
| Flux figé sur workers distants | [#105](https://github.com/remi-deher/flux-de-fichiers/pull/105) | Le bail porte `frozen_flow` (+ contexte *arr*) ; le worker distant exécute via `FlowEngine` |
| SSE progression distant | [#106](https://github.com/remi-deher/flux-de-fichiers/pull/106) | `job_progress` via `POST /api/v1/jobs/{id}/progress` puis SSE (même contrat que le worker intégré) |
| Palette + exécution encode plugins | [#108](https://github.com/remi-deher/flux-de-fichiers/pull/108) | Blocs plugins dans le catalogue (`GET /api/v1/flows/blocks`) ; `video/audio.encode` via FFmpeg (`tool.ffmpeg`, repli `FFMPEG`/PATH) |
| Versions de flux | [#110](https://github.com/remi-deher/flux-de-fichiers/pull/110) | `GET /api/v1/flows/{id}/versions[/{v}]` ; tiroir « Versions » de l'éditeur (restaurer puis enregistrer) |
| Défaut `encoder: auto` | `flux-de-fichiers-plugins` `main` | `plugins/video/blocks/video-encode.json` |

## Non vérifié

- Le client compile en WASM (CI et image `test-34f365e9af28`) et affiche son écran de connexion
  en production ; **aucun écran connecté n'a encore été ouvert**.
- Aucun parcours de bout en bout : connexion, création et relance d'une tâche, édition et
  enregistrement d'un flux, bibliothèques, plugins, comptes.
- Essai E2E NAS **non refait** après #105/#106/#108 : déployer une image `:test-<sha>` récente +
  `index.html` **et** `pkg/`.
- Les tests des PR récentes n'ont tourné que là où la CI s'applique (`dev → test`) ; les PR
  feature → `dev` n'ont pas de checks (ADR 0002). Exemple : #108 a laissé des `expect()` refusés
  par clippy, corrigés par `cc52dd6` avant la PR `dev → test` #109. Lancer
  `cargo clippy --workspace --all-targets -- -D warnings` avant de fusionner dans `dev`.
- Jusqu'à #104, le filtre de chemins de `test.yml` ignorait `client/` et `shared/` : une PR qui
  ne touchait que ces dossiers ne reconstruisait ni ne testait rien (image `test-24aaadc4aebe` =
  ancien WASM). Vérifier qu'une image contient le WASM attendu avant de la déployer.

## Limites connues / gaps techniques

**Interface**

- Gabarit d'origine repris dans le client (14 nov.) : barre latérale, barre du haut (recherche
  contextuelle), workers en ligne, `auth-gate`, sous-navigation, registres et tiroirs
  (`client/src/ui/`, `client/src/shell.rs`).
- Non repris : widgets personnalisables de la vue d'ensemble, feuille de filtres de la barre du
  haut (remplacée par les onglets), onglets Réseau / Stockage / Intégrations, télémétrie
  matérielle, page Mises à jour, import de flux depuis le catalogue (route inexistante),
  simulation animée.
- Éditeur de flux : pas de glisser-déposer, pas d'avertissement en quittant avec des
  modifications. Restaurer une version garde les en-têtes secrets du flux courant (masqués).
- Tâches : pas de choix du worker à la création ; 200 tâches au plus, sans pagination.
- Session expirée : message d'erreur ; retour à la connexion seulement en rechargeant.
- Pas d'écran « mot de passe oublié » ; rotation de clé en CLI ; pas de changement de mot de
  passe sur un worker.
- Accessibilité non auditée ; sans JavaScript ni WebAssembly, aucune interface.

**Moteur et plugins**

- `block.video.remux-mp4` (MP4Box) encore traversé, pas exécuté.
- Artefacts FFmpeg du catalogue encore `contract-only` (besoin de binaires réels ou `FFMPEG`).
- Pas de fallback GPU → CPU automatique.
- Pas de runner argv déclaratif générique (mapping kind encore en Rust).
- Secrets webhook / Arr déchiffrés voyagent dans le bail (à durcir si besoin).
- Baux sans numéro d'époque (*fencing*) : un ancien worker peut encore répondre tard.
- Requêtes SQL non vérifiées à la compilation (`query!` non adopté, voir l'audit).
- `frontend/dist/index.html` est versionné : le régénérer (`cargo run -p flux-frontend`) après
  toute modification du thème ou de la page.

## Suite proposée (TODO)

### Produit — à coder (Remi a dit « à faire »)

1. **Écrans UX manquants** (prioriser avec Remi si le scope est trop large)
   - Recherche globale
   - Navigation mobile (aujourd'hui barre du haut seulement)
   - Autres trous listés ci-dessus (widgets overview, onglets paramètres, télémétrie, etc.)

### Produit — à cadrer avec Remi avant de coder

2. **Époque des baux (fencing)** — éviter qu'un ancien worker réponde tard sur une tâche reprise
3. **IA et MCP** — dans le périmètre ADR 0012, non commencés ; écrire un ADR d'abord

### Hygiene / optionnel

4. Aligner `.cursorrules` / `.cursor/rules` et le début d'ADR 0014 / `developpement.md` sur la
   stack WASM actuelle (sinon les agents reprennent une fausse base).
5. Essai NAS après promotion `dev → test` (image `:test-<sha>` + frontend).

## Ordre suggéré pour un autre agent

1. Confirmer que #105, #106, #108 sont bien sur `dev` (et sync local DELL).
2. Enchaîner **Écrans UX** (sous-tâches claires : versions flux → recherche → mobile).
3. Ne pas démarrer époque des baux ni IA/MCP sans atelier avec Remi.
4. Optionnel : PR `dev → test`, essai NAS, hygiene règles Cursor.

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
