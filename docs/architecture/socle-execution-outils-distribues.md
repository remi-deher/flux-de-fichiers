# Socle d'exécution pour le moteur Bare Core et les outils distribués

Date : 9 novembre 2026. Statut : architecture technique cible (ADR 0004).

## 1. Vue d'ensemble du socle

Le socle d'exécution concrétise la décision utilisateur du 9 novembre 2026 (**Option B : Bare Core et outils distribués**) à travers une articulation claire entre le dépôt applicatif `flux-de-fichiers` et le catalogue déclaratif `flux-de-fichiers-plugins`.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       FLUX-DE-FICHIERS (Application)                        │
│                                                                             │
│  ┌───────────────────────┐                    ┌──────────────────────────┐  │
│  │ Coordinateur Central  │                    │ Worker Node (Nœud Agent) │  │
│  │ - API Axum / TLS      │                    │ - Moteur d'exécution nu  │  │
│  │ - Baux PostgreSQL     │◄── HTTPS + Token ──┤ - Runner de Macro-Blocs  │  │
│  │ - Ordonnancement      │                    │ - Cache local d'outils   │  │
│  │   tenant compte de    │                    │ - Sonde de télémétrie    │  │
│  │   la télémétrie       │                    │   matérielle native      │  │
│  └───────────────────────┘                    └────────────┬─────────────┘  │
└────────────────────────────────────────────────────────────┼────────────────┘
                                                             │ Résolution &
                                                             │ téléchargement
                                                             ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     FLUX-DE-FICHIERS-PLUGINS (Catalogue)                    │
│                                                                             │
│  - Schémas JSON (`tool-distribution.v1`, `macro-block.v1`, `block.v1`)      │
│  - Manifestes déclaratifs (`tools/mkvtoolnix`, `tools/ffmpeg`)              │
│  - Fiches de distribution par architecture (x86_64, aarch64) avec SHA-256   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Documents de conception de référence

Cette architecture est détaillée dans quatre documents complémentaires :

1. **[ADR 0004 — Moteur Bare Core, distribution d'outils par plugins (Option B) et socle d'exécution](../adr/0004-bare-core-et-outils-distribues.md)** :
   Formalisation de la décision utilisateur, motivation, périmètre et règles fondamentales.
2. **[Conception technique de la sonde système de télémétrie](sonde-telemetrie-systeme.md)** :
   Spécification de l'outil interne unique (`tool.internal.telemetry`), relevé des métriques CPU/RAM/GPU/Stockage sans privilèges root et intégration au scheduling du coordinateur.
3. **[Conception technique du cache worker et validation SHA-256 des outils distribués](cache-worker-outils-distribues.md)** :
   Cycle de vie d'acquisition, extraction native Rust sans shell (`noShellUnpack: true`), vérification cryptographique obligatoire SHA-256 et confinement d'exécution.
4. **[Conception technique de l'ordonnancement des macro-blocs](ordonnancement-macro-blocs.md)** :
   Modèle d'exécution composite au sein d'une tâche worker unique, isolation sandbox, passage des données et politique d'erreur stricte (`errorPolicy: stop`).

---

## 3. Matrice de conformité aux critères d'acceptation

| Critère d'acceptation | Référence documentaire | Statut |
| :--- | :--- | :--- |
| **1. ADR 0004 rédigé et conforme aux décisions utilisateur** | `docs/adr/0004-bare-core-et-outils-distribues.md` | Conforme |
| **2. Conception technique de la sonde système interne (CPU/RAM/GPU/disques)** | `docs/architecture/sonde-telemetrie-systeme.md` | Conforme |
| **3. Conception du cache worker et validation SHA-256 des binaires d'outils** | `docs/architecture/cache-worker-outils-distribues.md` | Conforme |
| **4. Modèle d'ordonnancement des macro-blocs documenté** | `docs/architecture/ordonnancement-macro-blocs.md` | Conforme |
| **5. Respect strict des règles de build/tests sans exécution lourde locale** | Aucune compilation Docker lourde ni suite de tests lancée en local | Conforme |

---

## 4. Trajectoire d'implémentation future (Phase de code)

Une fois cette phase de cadrage et de conception validée par le coordinateur et l'utilisateur, l'implémentation applicative pourra se déployer selon les étapes suivantes :

1. **Module `telemetry` côté Rust** : implémentation dans `src/telemetry.rs` des structures et de la lecture non bloquante de `/proc` et `/sys`.
2. **Migration du schéma de base de données** : ajout des colonnes ou de la table dédiée au stockage des métriques instantanées remontées lors des heartbeats des workers.
3. **Gestionnaire de cache `tools_cache`** : implémentation de la vérification SHA-256, du verrouillage inter-processus et de l'extraction sans shell dans `src/tools/cache.rs`.
4. **Exécuteur de macro-blocs** : implémentation de la boucle séquentielle d'exécution des étapes, de l'isolation sandbox et de la politique `errorPolicy: stop`.
5. **Nettoyage du Dockerfile** : retrait du paquet `mkvtoolnix` de l'image Docker de production (`debian:bookworm-slim`), transformant formellement le binaire en véritable Bare Core.
