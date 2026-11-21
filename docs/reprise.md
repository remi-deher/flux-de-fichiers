# Reprise du chantier — état au 14 nov. 2026 (après #105/#106/#108)

Note de passage pour continuer avec un autre agent ou sur une autre machine. Les décisions
durables sont dans les ADR ; ce document dit ce qui est **livré**, ce qui n'est pas vérifié et
ce qui **reste à faire**. À mettre à jour (ou supprimer) quand l'état change.

## À lire d'abord

- `AGENTS.md` (règles permanentes), puis `docs/developpement.md` (où est le code, quoi vérifier).
- ADR récents : [0012](adr/0012-perimetre-perte-de-bail-et-versions-de-flows.md) (périmètre,
  perte de bail, versions de flux), [0013](adr/0013-vocabulaire-central-worker.md) (vocabulaire
  central / worker), [0014](adr/0014-client-leptos-wasm.md) (client Leptos WASM),
  [0015](adr/0015-ia-et-mcp.md) (IA et MCP, proposé).
- Analyse du projet de référence `encoding-central` :
  [audit du 13 nov.](audits/2026-11-13-analyse-encoding-central.md).

## Règles de travail à respecter

- **Ni compilation ni test en local, sauf demande explicite** (machine faible, décision du
  20 nov.) : ni `cargo build`/`check`/`test`, ni clippy, ni build Docker ; fmt, clippy, la suite
  complète et l'image tournent en CI
  (`feature.yml` sur les PR vers `dev`, `test.yml` sur les PR `dev → test`, ADR 0002).
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
| `.` (`forgearr`) | Central et worker : API Axum, SQLite, moteur de flux, transferts |
| `shared/` (`forgearr-shared`) | **Contrat** lu par le central et le client : tâches, flux (graphe, catalogue des blocs exécutables), bibliothèques, workers, plugins, comptes, session, système, chemins |
| `client/` (`forgearr-client`) | Interface Leptos `csr` en WASM, un dossier par écran |
| `frontend/` (`forgearr-frontend`) | Page `dist/index.html` et thème CSS typé ; rien d'autre |

Écrans du client : accès et installation, vue d'ensemble, tâches, flux (éditeur), bibliothèques,
workers, plugins, paramètres. Sur un worker : vue d'ensemble, tâches et plugins (lecture),
paramètres.

## Livré (fusionné sur `dev`)

| Sujet | PR / dépôt | Contenu |
|---|---|---|
| Flux figé sur workers distants | [#105](https://github.com/remi-deher/flux-de-fichiers/pull/105) | Le bail porte `frozen_flow` (+ contexte *arr*) ; le worker distant exécute via `FlowEngine` |
| SSE progression distant | [#106](https://github.com/remi-deher/flux-de-fichiers/pull/106) | `job_progress` via `POST /api/v1/jobs/{id}/progress` puis SSE (même contrat que le worker intégré) |
| Palette + exécution encode plugins | [#108](https://github.com/remi-deher/flux-de-fichiers/pull/108) | Blocs plugins dans le catalogue (`GET /api/v1/flows/blocks`) ; `video/audio.encode` via FFmpeg (`app.ffmpeg`, repli `FFMPEG`/PATH) |
| Versions de flux | [#110](https://github.com/remi-deher/flux-de-fichiers/pull/110) | `GET /api/v1/flows/{id}/versions[/{v}]` ; tiroir « Versions » de l'éditeur (restaurer puis enregistrer) |
| Remux MP4 (MP4Box) | [#114](https://github.com/remi-deher/flux-de-fichiers/pull/114) | `block.video.remux-mp4` exécuté (`app.mp4box`, repli `MP4BOX`/PATH) ; exécuteur partagé `tool_run` (erreurs typées `ToolRunError`, progression lue sur `\r`/`\n`) |
| Repli GPU → CPU | [#133](https://github.com/remi-deher/flux-de-fichiers/pull/133) | Échec FFmpeg avec encodeur matériel : repli journalisé et une seule retentative en CPU |
| Secrets du bail limités | [#134](https://github.com/remi-deher/flux-de-fichiers/pull/134) | Le bail ne porte que les connexions *arr* des blocs du flux figé |
| Sections du catalogue | branche `feat/catalogue-sections-et-proprietaire` | Page Plugins : Applications, Plugins, Flows, Autre, Packs, Dépôts ; blocs jamais exposés par `GET /api/v1/plugins` ; `owner` obligatoire pour un bloc (voir [deux-depots](architecture/deux-depots.md)) |
| Refonte du dépôt plugins | `flux-de-fichiers-plugins` branche `refonte/types-et-proprietes` | `tools/` → `apps/` (`app.*`), `app.media` → `pack.media`, blocs sous leur plugin, exemples sous `examples/catalogue/`. À fusionner avec la branche du moteur |
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
  par clippy, corrigés par `cc52dd6` avant la PR `dev → test` #109. Depuis #112, `feature.yml`
  lance clippy sur les PR vers `dev` : attendre son résultat avant de fusionner.
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
  matérielle, page Mises à jour, simulation animée. Les modèles de flows (locaux et issus du
  catalogue) se créent et s'utilisent dans Librairies → Flows & Modèles (`/api/v1/local/flow-templates`).
- Éditeur de flux : glisser-déposer sans position libre (mise en page automatique) ; non essayé
  en réel (HTML5 DnD, `beforeunload`). Restaurer une version garde les en-têtes secrets du flux courant (masqués).
- Aucun de ces écrans ni leurs correctifs (recherche globale, tiroir mobile, session expirée, choix du worker, pagination) n'a été ouvert en réel : seuls clippy et les tests natifs ont tourné.
- « Mot de passe oublié » par courriel SMTP (ADR 0015) : écrit et couvert par des tests natifs, jamais
  essayé contre un vrai relais SMTP ni dans le navigateur. Les comptes existants n'ont pas d'adresse :
  un administrateur la renseigne (Paramètres > Utilisateurs, bouton « E-mail »). Rotation de clé en CLI ;
  pas de changement de mot de passe sur un worker (le worker ne propose pas le lien : ses comptes sont
  une copie du central, il faudrait une route de relais vers le central).
- Accessibilité non auditée ; sans JavaScript ni WebAssembly, aucune interface.

**Moteur et plugins**

- Le remux MP4 est exécuté par `MP4Box -add … -new` ; le fichier de sortie reste validé par `mkvmerge` à la publication (à vérifier en réel sur un `.mp4`).
- Le bail porte encore le clair des secrets nécessaires au flux (le worker n'a pas la clé) ; seul un chiffrement par clé de worker changerait cela.
- `binaryRelativePath` des fiches de distribution (`bin/MP4Box`, `bin/ffmpeg`, `bin/mkvmerge`) contient déjà `bin/` alors que `tools_cache` l'ajoute : à vérifier quand les artefacts réels existeront.
- Artefacts FFmpeg du catalogue encore `contract-only` (besoin de binaires réels ou `FFMPEG`).
- Pas de fallback GPU → CPU automatique.
- Pas de runner argv déclaratif générique (mapping kind encore en Rust).
- Secrets webhook / Arr déchiffrés voyagent dans le bail (à durcir si besoin).
- Baux sans numéro d'époque (*fencing*) : un ancien worker peut encore répondre tard.
- Requêtes SQL non vérifiées à la compilation (`query!` non adopté, voir l'audit).
- `frontend/dist/index.html` est versionné : le régénérer (`cargo run -p forgearr-frontend`) après
  toute modification du thème ou de la page.

## Reste à faire (liste complète au 14 nov.)

Chaque point est indépendant sauf mention. Cocher ou retirer au fil de l'eau.

### 0. En cours

- [x] PR [#111](https://github.com/remi-deher/flux-de-fichiers/pull/111) `dev → test` fusionnée
      (CI verte, image `:test-67cb9cc…` publiée).
- [ ] Déployer cette image sur les NAS (procédure dans « Production » ci-dessous ; **vérifier
      d'abord que le WASM de l'image est le bon**, voir « Non vérifié »).

### 1. Vérifier en réel

Reporté (décision du 14 nov.) : on ne corrige pas pour le moment, on le fera plus tard.

- [ ] Parcours connecté de chaque écran sur l'essai (`http://192.168.1.50:18480`).
- [ ] Tâche réelle de bout en bout **sur le worker distant** (flux figé #105).
- [ ] Bloc d'encodage plugin réel (#108) : nécessite un vrai FFmpeg.
- [ ] Expliquer les avertissements `slow statement` / `slow acquire` SQLite du 14 nov. vers 09:10.

### 2. Interface (client WASM, `client/src/`)

- [x] Recherche globale (#115) : tâches, flux, bibliothèques, workers, plugins ; le résultat mène à l'objet (éditeur du flux, ou écran filtré). Les listes sont lues à l'ouverture du champ, sans route dédiée ; les tâches se limitent à la première page de 200.
- [x] Navigation mobile (#116) : barre latérale en tiroir sous 768 px. Non vérifiée sur un vrai écran.
- [x] Widgets de la vue d'ensemble : choix des widgets affichés (bouton « Personnaliser »), préférence gardée dans le navigateur.
- [x] Paramètres : onglets Réseau (écoute, TLS, origine, session) et Stockage (dossiers, taille maximale, espace du volume), lus dans `/api/v1/system/status`.
- [x] Réseau, lots A, B et C : carte Configuration éditable (`PUT /api/v1/system/settings` pour origine publique `WEB_ORIGIN`, TTL des sessions web, messagerie SMTP avec mot de passe chiffré en base et politique de proxy/requêtes sortantes privées), connectivité des workers (dernier contact, ancienneté, version, adresse réseau vue par le central, télémétrie CPU/RAM/stockage/GPU, alerte au-delà de `WORKER_SILENT_ALERT_SECONDS`) et diagnostic `POST /api/v1/system/diagnostics`.
- [x] Télémétrie matérielle du central (`GET /api/v1/system/hardware`) et des workers distants (persistée à l'enregistrement, sur heartbeat et en attente de bail, affichée dans les fiches et détails worker).
- [x] Paramètres (14 nov.) : onglet « Stockage » retiré (taille maximale d'un fichier et espace libre déplacés dans « Système ») ; « Mises à jour » fusionné dans « Système ». Ancienne page Mises à jour : version du central et de l'interface, procédure. Limite : pas de « version disponible », le central n'appelle aucun service externe (une vérification facultative reste à décider).
- [x] Feuille de filtres de la barre du haut : période des tâches (Activité) et présence des workers ; remise à zéro au changement d'écran. Les autres écrans n'ont pas de critère propre.
- [x] Simulation à sec dans l'éditeur (`client/src/flows/simulation.rs`) : évaluation locale fidèle des conditions (`video.codec`, `audio.language`, etc.), profils de média et navigation pas à pas sur le graphe.
- [x] Éditeur de flux : glisser-déposer palette → carte et carte → carte (`flows/dnd.rs`, règles de `FlowGraph::connect`, alternative clic/liste conservée) ; confirmation avant de quitter avec des modifications (routeur, retour arrière, `beforeunload`).
- [x] Tâches : choix du worker à la création (#118) ; pagination au-delà de 200 (#119) ; filtrage par bibliothèque, métadonnées entrée/sortie et journal des étapes.
- [x] Session expirée : un 401 sur session ouverte recharge la page, qui affiche la connexion (#117).
- [x] « Mot de passe oublié » par courriel SMTP (ADR 0015) ; la CLI `reset-password` reste le recours.
- [x] Changement de mot de passe depuis un worker (`PUT /api/v1/account/password` relayé vers `PUT /api/v1/worker/account/password` sur le central).
- [x] Passe d'accessibilité (`Drawer` modal avec `aria-modal`, `aria-labelledby`, focus automatique et fermeture `Échap`, étiquettes ARIA et styles `:focus-visible`).
- [x] Librairies locales autonomes (`src/plugins/local/`, `client/src/libraries/`) : scripts locaux (`stdio` JSON avec `ScriptInterpreter` typé, catégorie, étiquettes, starters et bloc palette dédié), import/remplacement/téléchargement de ressources (`/api/v1/local/items/{id}/file`), modèles de blocs (édition, inspection, insertion en palette sans chevauchement avec `branch_target` / `palette_pinned`), modèles de flows (`FlowsSection`) et packs locaux assemblés avec tiroir d'inspection/installation (`<PackDetail>`).

### 3. Moteur et plugins

- [x] Exécuter `block.video.remux-mp4` (MP4Box) (#114).
- [x] Identifiants du catalogue sans préfixe de type (`ffmpeg`, `essentiel`, `video.encode`… ; seules les opérations gardent `operation.*`), date `updatedAt` par pièce et `releasedAt` par outil affichées dans Catalogue/Outils, retrait de la pastille « Dépôts » du Catalogue, installation des outils requis par un plugin (`POST /api/v1/plugins/{id}/install`, dépendances transitives, aussi appliquée à `POST /api/v1/tools/{id}/install`). Les réglages et secrets enregistrés sous les anciens identifiants sont repris automatiquement (migration `0027` + rescellement au démarrage, `plugins::legacy_ids`), de même que les types de blocs des flux en base, de leurs versions figées et des modèles locaux.
- [x] Contrats d'opération étendus pour les outils composés : `pathTools` (outils du cache en tête du `PATH`, comptés parmi les outils requis), chemin `reference` / `sourceFile` (original de la tâche), `acceptsNonMedia` (entrée non média tolérée, nouvelle sonde quand un script change de fichier). Côté catalogue : Dolby Vision et HDR10+, archives (7-Zip), resynchronisation (alass) et OCR PGS (Tesseract) des sous-titres, tags MP4 (AtomicParsley), qualité VMAF (mesure et encodage à qualité cible avec ab-av1), jq dans les interpréteurs.
- [x] Artefacts `ffmpeg` / `ffprobe` / `MP4Box` / `mkvtoolnix` / `mediainfo` intégrés dans `.github/workflows/build-tools.yml` (côté `flux-de-fichiers-plugins`).
- [x] Repli automatique GPU → CPU quand l'encodeur matériel échoue (#133).
- [x] Moteur 100 % déclaratif par contrats ([ADR 0017](adr/0017-moteur-generique-et-contrats.md)) : `OperationRegistry`, exécuteurs génériques `binary_runner` / `http_runner` et `script_runner` (`kind: "script"`, routage `nextPort`, `workingFile`, fusion `mediaInfo`, `Skip`, `stopPipeline`), installation déclarative de packs. Côté `flux-de-fichiers-plugins` : schémas, `build_index.py`, `check_catalogue.py` et `scaffold.py script` alignés sur `type: "script"`.
- [x] Variables de flux (`shared/src/flows/variables.rs`, `src/flows/substitution.rs`) : variables intrinsèques (`file.*`, `source.name`, `library.*`, `worker.name`, `job.id`, `flow.id`) et personnalisées `vars.*`, modèles `{$expr|défaut}` (échappement `{{$`) rendus dans la configuration des blocs binaires, scripts et primitives, bloc `set-variable`, conditions sur `vars.*` / `file.*`, protocole script `1.1` (`variables` en entrée et en sortie) et transmission des variables au relais (migration `0036`).
- [x] Conditions composées (`shared/src/flows/condition/`) : opérateur typé `ConditionOperator` (`eq` / `neq` historiques = « contient » / « ne contient pas », nouveaux `not_exact`, `regex`, `in`), groupes ET/OU imbriqués sur `MAX_CONDITION_DEPTH` niveaux (`group`, format plat conservé), validation à l'enregistrement (profondeur, nombre de règles, regex) et éditeur dédié dans l'inspecteur (`condition_editor.rs`). Hors périmètre : unification avec `ConditionClause` des contrats.
- [x] Chiffrement par clé de worker et par bail des secrets transmis sur `/api/v1/claims` (`Claim.sealed_secrets`, dérivation `SecretBox::for_worker_lease` liée à `(job_id, lease_epoch)`).
- [x] Requêtes SQL vérifiées à la compilation (`sqlx::query!` / `query_as!` / `query_scalar!` + cache hors-ligne `.sqlx/` pour `Dockerfile` avec `SQLX_OFFLINE=true`).
- [x] Automatisation des bibliothèques (scans planifiés, extensions incluses, motifs exclus, taille minimale, politique de sortie), gestion avancée des tâches (annulation active, lots, priorités, concurrence et plages horaires des workers, statistiques d'économies disque), purge automatique du cache d'outils, test de connexion des plugins API et vérification facultative des mises à jour GitHub.

### 4. À cadrer avec Remi avant de coder

- [x] Enrôlement par demande à approuver et fenêtre d'ouverture de 15 minutes : [ADR 0016](adr/0016-enrolement-par-approbation.md) implémenté.
- [x] Époque des baux (*fencing*) : `jobs.lease_epoch` vérifié sur `/heartbeat`, `/progress`, `/steps`, `/failure`, `/source` et `/result` (`409 Conflict` si incohérent).
- [x] IA et MCP ([ADR 0015](adr/0015-ia-et-mcp.md)) : serveur Streamable HTTP sur `/mcp` (outils complets dont `simulate_flow`, `diff_flow_versions`, `diagnose_job`, `annotations`, ressources système/workers/stats, `prompts/list` + `prompts/get`, extrait `.mcp.json` dans Paramètres > Clés d'API), sans file d'approbation humaine, avec journal d'audit persistant (`mcp_audit_log`, `GET /api/v1/mcp/audit`) et limitation de débit (120 req/min) ; assistant LLM intégré dans l'interface web (`Ollama`, `OpenAI / API compatible`, `Anthropic Claude`, clé chiffrée au repos via `SecretBox`, `Paramètres > Assistant IA`, tiroir global et raccourcis contextuels tâches/flux branchés sur les outils MCP).

### 5. Hygiène et process

- [x] CI sur les PR feature → `dev` : `.github/workflows/feature.yml` (fmt, clippy natif et WASM,
      tests, page générée à jour), ADR 0002 mis à jour. Reste à rendre `fmt-clippy-tests`
      obligatoire sur `dev` (réglage GitHub, hors dépôt).
- [x] `.cursorrules` / `.cursor/rules` alignés sur la stack actuelle.
- [x] Release `test → main` : aucune pour le moment (décision du 14 nov.).
- [x] `frontend/dist/index.html` reste versionné et régénéré à chaque changement ;
      `feature.yml` échoue si la page est périmée.

## Ordre suggéré pour un autre agent

1. Lire « À lire d'abord » et « Règles de travail » ; `git pull` sur `dev`.
2. Finir le point 0 (CI #111, déploiement NAS) si ce n'est pas fait.
3. Demander à Remi le retour du parcours connecté (1) et corriger ce qu'il révèle.
4. Interface (2) : recherche globale, puis navigation mobile, puis le reste.
5. Moteur (3) selon les besoins de l'encodage réel.
6. Ne pas commencer le point 4 sans atelier avec Remi.

## Production (NAS)

**Déployé le 14 nov. 2026** : `mrcryllix/forgearr:test-0c6de4f0a632` (PR #109 : gabarit
d'origine, #105, #106, #108) sur les deux hôtes. Vérifié : santé, WASM au bon contenu et sans
cache, worker enregistré, porte d'accès affichée. Non vérifié : écrans connectés. Sauvegardes et
retour arrière : journal de Rashin du 14 nov. (`*.avant-0c6de4f*`).

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
