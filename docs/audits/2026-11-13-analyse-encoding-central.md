# Analyse du projet de référence `encoding-central` et comparaison avec la réécriture

Date : 13 nov. 2026. Source : `github.com/remi-deher/encoding-central`, branche par défaut,
commit `e3c2942` (8 nov. 2026). Lecture du code, des ADR (0001 à 0048), des fiches de domaine
`docs/architecture/domains/*` et des règles (`AGENTS.md`, `BACKEND_RULES.md`, `UI_RULES.md`,
`TEST_RULES.md`). Rien n'a été compilé ni exécuté.

## 1. Vue d'ensemble chiffrée

| | encoding-central (référence) | flux-de-fichiers (réécriture) |
|---|---|---|
| Backend | C# ASP.NET Core .NET 10, ~46 500 lignes, 5 projets | Rust Axum/Tokio, ~13 200 lignes (`src/` + `shared/`) |
| Frontend | Angular 22 + Material 3, ~60 000 lignes TS, ~9 800 SCSS, ~5 000 HTML | Leptos, ~12 800 lignes Rust (thème compris) |
| Routes HTTP | ~300 (`Map*` : 306 occurrences) | 52 chemins `/api/v1/*` |
| Migrations SQLite | ~77 (`0001` → `0077`) | 11 |
| ADR | 48 + fiches de domaine | 11 |
| Tests | ~20 600 lignes .NET, 83 specs Angular, 16 E2E Playwright, tests Docker multi-conteneurs, stress | 317 tests Rust (~4 200 lignes dans `tests/`) |
| Déploiement | 2 images (central, worker) | 1 image, rôle choisi à l'installation (ADR 0011) |
| Outils média | FFmpeg, ffprobe, MediaInfo, MKVToolNix dans l'image, GPU (NVENC, QSV, VA-API) | « Bare core » : outils distribués par plugins, SHA-256 (ADR 0004, 0007) |

Point notable : l'ADR 0048 de la référence (8 nov. 2026) avait déjà décidé une réécriture du
**backend seul** en Rust, progressive (worker d'abord, puis central en une bascule), en gardant
Angular, le même fichier SQLite, le même schéma et un contrat HTTP figé. flux-de-fichiers a pris
une autre voie : réécriture **complète** (backend + interface), base neuve, contrat neuf.

## 2. Backend de la référence

### 2.1 Organisation

- `Encoding.Contracts` (4 900 l.) : DTO et contrats versionnés partagés API ↔ worker.
- `Encoding.Core` (1 400 l.) : journalisation, runtime commun, outils, plugins.
- `Encoding.Api` (30 700 l.) : le central. Plus gros domaines : `Executions` (5 800), `Tools`
  (4 900), `Flows` (4 800), `Plugins` (3 300), `Mcp` (3 200), `Infrastructure` (2 900),
  `Workers` (2 000), `Libraries`, `Auth`, `Resources`, `Logging`, `Updates`.
- `Encoding.WorkerAgent` (9 500 l.) : agent, dont `Runtime` (7 300 l.) pour l'exécution des étapes.
- `Encoding.Launcher` : lanceur.
- Garde-fous d'architecture par tests (`ArchitectureTests`) : couches de projets, plafond de
  600 lignes par fichier, cliquets sur les méthodes de plus de 120 lignes et sur la dette connue
  (29 endpoints qui touchent encore un dépôt, 3 `new HttpClient()`).

### 2.2 Fonctionnalités techniques

- **Stockage** : SQLite unique sur le central, écrivain unique avec file à priorités
  (présence et baux prioritaires), `BEGIN IMMEDIATE`. SQL d'abord écrit en dialecte PostgreSQL
  puis traduit par regex, retiré par l'ADR 0043. Incident #32 : la validation de session ouvrait
  une transaction d'écriture à chaque requête, d'où des 503 `sqlite_write_unavailable`.
- **Workers** : enrôlement par compte admin du central ou par clé de jumelage (10 min, usage
  unique), découverte mDNS `_encoding._tcp`, jeton propre au worker (SHA-256 côté central,
  `X-Node-Token`), version exacte imposée (ADR 0039), groupes et étiquettes, détection du
  matériel et des encodeurs vérifiés, télémétrie par WebSocket (ADR 0045), réveil par SSE (ADR 0041).
- **Baux** : époque et *fencing*, refus d'étape avec codes stables (`lease_epoch_mismatch`…),
  **aucune réattribution silencieuse** : la perte du worker fait échouer la tentative
  (`worker_lost`), la reprise est une nouvelle exécution (manuelle ou bloc borné
  `nodes-retry-on-worker-loss`).
- **Accès aux médias** : chaque worker déclare par bibliothèque un accès direct, non pris en
  charge ou par transfert (WebDAV depuis un worker à accès direct, contrôle d'empreinte).
- **Intégrité** : scan avant toute transformation et scan de sortie avant publication, sortie
  temporaire validée, publication sans écraser la source, remplacement de la source seulement si la
  politique de bibliothèque l'autorise (désactivée par défaut, sauvegarde `.encoding-replaced/`,
  délai de grâce), verrou d'écriture source, garde de publication fournie par plugin (Plex :
  pas de lecture en cours).
- **Isolation** : scripts, plugins tiers et CLI IA dans un processus séparé sous Landlock et
  seccomp, réseau seulement avec la capacité `network`, aucun repli non confiné.
- **Secrets** : ASP.NET Data Protection (7 *purposes*), trousseau XML **en clair sur disque**
  (relevé par l'ADR 0048 comme faiblesse).
- **Exploitation** : métriques, journaux système en tampon circulaire avec masquage des secrets,
  rétention des artefacts, nettoyage des espaces de travail, page Mises à jour, OpenAPI généré.

### 2.3 Fonctionnalités métier

- **Bibliothèques** : définition logique (vidéo, image, musique, sous-titres), zéro ou un flow
  actif référencé par **version publiée**, découverte et surveillance des fichiers.
- **Flows** : DAG versionné, une entrée, chemins alternatifs séquentiels, validation backend seule
  autorité (1 000 blocs, 4 000 liaisons, 1 Mo). Versions publiées immuables, brouillons à
  révisions, sauvegardes de brouillons, import/export JSON et ZIP, clonage, suppression logique.
- **Simulation** sans effet externe, avec résultats supposés explicites pour les blocs externes.
- **Ressources réutilisables** : scripts JS/Python versionnés, modèles de flow à paramètres
  guidés, packs de composants, profils d'encodage (choix automatique NVENC > QSV > VA-API > CPU),
  ressources fichiers (polices, profils et fichiers de sous-titres).
- **Création guidée vidéo** et *builder* FFmpeg explicite (Début → réglages → Exécuter →
  publication), assemblage MKV, contrôle d'intégrité optionnel par décodage.
- **Plugins et Store** : dépôt public, manifestes déclaratifs versionnés, empreintes SHA-256,
  état désiré des workers = paquets installés sur le central. Paquets natifs `base`, `nodes`,
  `conditions`, `video`, `audio`, `subtitles`, `mkv`, `images`, `probe`, `ai` ; plugins Python
  Plex, Jellyfin, Sonarr, Radarr ; webhooks entrants Sonarr/Radarr.
- **IA et MCP** : serveur MCP (lecture par défaut, écriture par portée), assistant
  conversationnel, génération de flows, traduction de sous-titres, enrichissement de métadonnées,
  connexion OpenAI, Anthropic et Claude CLI.
- **Comptes** : setup imposé, rôles `admin` et `viewer`, jetons API à portées
  (`flows:write`, `libraries:write`…), CSRF par en-tête `X-Encoding-Csrf`, limitation des
  requêtes anonymes, jeton de setup optionnel, route **fermée par défaut** avec test qui
  énumère les routes anonymes.

### 2.4 État réel déclaré par le projet lui-même

`open-questions.md` et `docs/reprise.md` le disent : jamais vérifiés en conditions réelles,
Landlock en production, GPU réels, Plex/Jellyfin/*arr réels, NAS/NFS. Le lot de migrations
0068 à 0074 et la CI de promotion n'avaient pas tourné. Le worker n'avait pas encore ses pages ;
le build `worker` remplaçait `app.routes.ts` par une copie (risque de dérive identifié).

## 3. Frontend de la référence

- **Structure** : `core/` (auth, layout, navigation, shell, thème), `features/` par domaine
  (activity, ai, auth, flows, home, libraries, plugins, settings, setup, updates, workers),
  `shared/` (models 6 000 l., services 3 100 l., ui 13 600 l., templates, guards, pipes).
- **Routes paresseuses** (`loadComponent`) dont les métadonnées (`data.navigation` : groupe,
  libellé, glyphe, ordre, mots-clés) alimentent à la fois la navigation et la recherche globale.
- **Shell adaptatif** : navigation 248 px repliable en rail 88 px, navigation basse sous 761 px,
  recherche globale contextuelle (`PageShellContextService`), lanceur d'assistant IA flottant.
- **Composants communs** : `DataTable` (table ou cartes), `RecordForm` et formulaire de schéma,
  `DetailDrawer` (feuille basse sous 700 px), `ResourceCard` / `ResourceSheet`, `MetricCard`,
  `EmptyState`, `CategoryTabs`, `Subnav`, éditeur d'arbre de conditions, éditeur de code.
- **Éditeur de flow** : canevas libre avec panoramique, zoom, minicarte, disposition automatique
  annulable, glisser-déposer CDK, cartes adaptatives mesurées (`ResizeObserver`), liaisons SVG
  routées, packs dépliables, panneau de simulation, assistant IA intégré.
- **Règles UI** : Material 3 Expressive, Signals uniquement (`input()`, `computed()`…), `OnPush`,
  contrôle de flux natif, jetons de design (surfaces tonales, rayons, typographie, ressorts de
  mouvement), quatre palettes × clair/sombre, `prefers-reduced-motion` respecté.
- **Faiblesse structurelle** : les modèles TypeScript (`shared/models`) recopient à la main les
  contrats C# ; l'ADR 0048 prévoyait des tests de contrat et des E2E pour détecter les écarts de
  sérialisation (dates, énumérations, `null`).

## 4. Différences avec la réécriture

| Sujet | Référence | flux-de-fichiers |
|---|---|---|
| Périmètre média | Encodage (FFmpeg, GPU, profils de qualité) | README : « sans réencodage », remux `mkvmerge`, sonde MediaInfo ; mais ADR 0006 et le dépôt de plugins (`video`, `audio`, outil `ffmpeg`) préparent des blocs d'encodage. **À clarifier.** |
| Rôles | 2 images, rôle fixé par l'image | 1 image, rôle choisi à l'installation |
| Vocabulaire | Fixé : central / worker (ADR 0040), après un long nettoyage | Mélange : coordinateur, centrale, agent, nœud, worker (routes `/nodes/enroll`, `/node/status`, `/workers`) |
| Auth machine | Jeton par worker + enrôlement admin, mDNS, version exacte | Clé par agent (ADR 0003) et clés d'API de rôle worker ; pas de mDNS ni de contrôle de version |
| Perte de worker | Échec `worker_lost`, jamais de réattribution silencieuse | Bail expiré → retour en file tant que `attempts < MAX_ATTEMPTS` (3) : **réattribution automatique** |
| Accès aux fichiers | Accès direct / transfert WebDAV déclaré par worker et bibliothèque | Source et résultat diffusés par HTTP (`jobs/{id}/source`, `/output`), `MAX_FILE_BYTES` 512 Mio |
| Flows | Versions publiées immuables, brouillons, import/export, modèles, packs, scripts | Définition unique modifiable, pas de version publiée ni de brouillon |
| Plugins | Code tiers (Python) sous Landlock/seccomp | Plugins déclaratifs (blocs, réglages) ; actions *arr et webhook codées en Rust |
| Secrets | Data Protection, trousseau XML en clair | ChaCha20-Poly1305, `secret_key` 0600, rotation (`secret_rotation.rs`) |
| SQL | SQL manuel, vérifié par tests | `sqlx::query()` dynamique (39), aucune macro `query!` vérifiée à la compilation |
| Temps réel | SSE `/api/v1/live` + WebSocket télémétrie | SSE `/api/v1/jobs/events` |
| IA / MCP | Complet | Absent |
| UI | Angular Material, TS séparé des contrats | Leptos, types partagés par la crate `flux-shared` |
| Tests | Très fournis, E2E Playwright, multi-conteneurs | 317 tests Rust natifs, pas d'E2E navigateur |

## 5. Avantages de la réécriture

1. **Contrat unique de bout en bout** : `flux-shared` partage les types entre serveur et
   interface ; la classe de bugs « modèle TS désynchronisé du DTO C# » disparaît.
2. **Un binaire, une image, un port** : pas de runtime .NET ni de bundle Node en production,
   empreinte mémoire et taille d'image bien plus faibles, adapté au NAS. Le modèle *arr
   (ADR 0005) supprime la double image et la copie des routes pour le worker.
3. **Bare core** : l'image ne transporte plus FFmpeg et MKVToolNix ; les outils arrivent
   vérifiés par SHA-256 et installés par `rename` atomique (ADR 0007). Mises à jour d'outils
   découplées des releases.
4. **Secrets mieux protégés** dès le départ (AEAD, clé 0600, rotation) ; pas de dette
   Data Protection ni de protocole de réchiffrement à porter (ADR 0048 §3).
5. **Pas d'héritage** : ni traducteur PostgreSQL → SQLite, ni résidus du maillage, ni
   vocabulaire « cluster » à purger, ni 77 migrations.
6. **Surface d'attaque réduite** : plugins déclaratifs plutôt que code tiers, donc pas besoin
   immédiat de Landlock/seccomp.
7. **Sûreté mémoire et concurrence** vérifiées par le compilateur (Tokio, `Send`/`Sync`) pour
   les baux, transferts en flux et SSE.

## 6. Ce que la référence a appris et qu'il faut reprendre

Par ordre d'impact sur l'intégrité des médias et la sécurité :

1. **Pas de réattribution silencieuse** après perte de bail : aujourd'hui un bail expiré
   relance la tâche ailleurs (`src/repository/jobs.rs`). Inoffensif pour un remux vers `/work`,
   dangereux dès qu'une étape modifie en place (ADR 0008) ou appelle un service externe.
   Ajouter époque/*fencing* au bail et un code `worker_lost`.
2. **Scan de sortie et validation avant publication**, source préservée, remplacement
   seulement par politique explicite désactivée par défaut, garde de publication (lecture Plex).
3. **Lectures hors transaction d'écriture** (incident #32) : ne jamais valider une session dans
   une transaction d'écriture ; prévoir un écrivain unique si la charge l'exige.
4. **Route fermée par défaut** et test qui énumère les routes anonymes ; jeton de setup optionnel
   pour fermer la fenêtre d'installation sur un réseau non fiable (ADR 0011 la laisse ouverte).
5. **Versions de flows publiées immuables** et exécution liée à une version précise : sans
   cela, modifier un flow change le comportement des tâches en cours ou rejouées.
6. **Version exacte worker = central** (ADR 0039) dès qu'il existe des agents distants.
7. **Vocabulaire fixé tôt** : le renommage a coûté à la référence une campagne entière
   (ADR 0040, migrations 0068 à 0074).
8. **Requêtes vérifiées à la compilation** (`sqlx::query!` + `sqlx prepare`) : c'était la
   recommandation de l'ADR 0048 ; le dépôt n'en utilise aucune.
9. **Une seule table de routes UI avec profils** (central/worker) plutôt que deux copies.
10. **Composants UI de la référence à reprendre en premier** : `DataTable` (table/cartes),
    formulaire piloté par schéma, `DetailDrawer`, éditeur d'arbre de conditions, minicarte et
    disposition automatique du canevas, recherche globale alimentée par la table de routes.

## 7. Écarts fonctionnels restants (non portés)

IA et MCP ; scripts JS/Python ; modèles, packs et ressources de flow ; profils d'encodage et
GPU ; import/export de flows ; transfert WebDAV et accès par bibliothèque ; groupes et
étiquettes de workers ; mDNS ; télémétrie WebSocket ; métriques ; rétention des artefacts ;
E2E navigateur. Chacun demande une décision : porter, reporter ou abandonner.

## 8. Décisions prises (13 nov. 2026)

Consignées dans l'[ADR 0012](../adr/0012-perimetre-perte-de-bail-et-versions-de-flows.md) :
encodage par plugins ; bail expiré = échec et relance manuelle seulement ; flows versionnés,
chaque tâche fige sa version ; IA et MCP dans le périmètre ; installation neuve ; fenêtre
d'installation ouverte assumée. Vocabulaire unifié en central / worker ([ADR 0013](../adr/0013-vocabulaire-central-worker.md)) ; requêtes `query!` examinées sans
changement pour l'instant.
