# Changelog

## [0.9.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.8.1...v0.9.0) (2026-11-11)


### Features

* **ui, plugins:** subnavbar sur 4 sections et multi-depots (v0.9.0) ([d06be7f](https://github.com/remi-deher/flux-de-fichiers/commit/d06be7f6414c3a689a857b766b71eb57e832a3d0))
* **ui, plugins:** subnavbars sur 4 sections et gestion multi-depots ([89f1b38](https://github.com/remi-deher/flux-de-fichiers/commit/89f1b3841e7084ab578de7d25a7e7b5980e75dc1))

## [0.8.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.8.0...v0.8.1) (2026-11-11)


### Bug Fixes

* **db:** preservation du checksum sqlx de la migration 0005 et purge bare core via migration 0006 ([1b7286e](https://github.com/remi-deher/flux-de-fichiers/commit/1b7286eb5b58befc2fdffd92b32d0b9c1f4a2e55))
* **db:** preservation du checksum sqlx de la migration 0005 et purge bare core via migration 0006 ([5438990](https://github.com/remi-deher/flux-de-fichiers/commit/5438990af65edb6550d07507e4f701944427101a))
* **db:** purge bare core idempotente via migration 0006 ([c064c46](https://github.com/remi-deher/flux-de-fichiers/commit/c064c4612de6ccbaf6157589674dd9b319a111d9))

## [0.8.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.7.0...v0.8.0) (2026-11-11)


### Features

* **core:** transition bare core, distribution des flows par plugins et stabilisation des agents ([f672b9f](https://github.com/remi-deher/flux-de-fichiers/commit/f672b9f0a1dcb18986683c9513e382e304f1ac5f))
* **flows:** transition vers le modele bare core avec distribution par plugins ([cc581d6](https://github.com/remi-deher/flux-de-fichiers/commit/cc581d60fbac37f370f79a9df02b072636713729))


### Bug Fixes

* **ci:** verrouillage du cache cargo registry contre les acces concurrents dans buildx ([2b2377f](https://github.com/remi-deher/flux-de-fichiers/commit/2b2377f14b06ec383377194cd60cb514389627d6))
* **core:** suppression des donnees mockees et stabilisation des agents ([6d31bb9](https://github.com/remi-deher/flux-de-fichiers/commit/6d31bb9b998ea49f66e30f9c288844358a04496f))
* **core:** suppression des donnees mockees et stabilisation des agents ([601a5ca](https://github.com/remi-deher/flux-de-fichiers/commit/601a5cace3e51c7abfea49f2d6cb8455237f440b))

## [0.7.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.6.0...v0.7.0) (2026-11-11)


### Features

* **frontend:** nouveau shell persistant avec navigation rail inspire d'encoding-central ([e4137e7](https://github.com/remi-deher/flux-de-fichiers/commit/e4137e7a057d58ac5e67be148b78a6949da4a277))
* **frontend:** nouveau shell persistant avec navigation rail inspire d'encoding-central ([3bd4ae3](https://github.com/remi-deher/flux-de-fichiers/commit/3bd4ae37affedc4e47e5ca540364e7647f219005))
* **frontend:** nouveau shell persistant et navigation rail inspire d'encoding-central ([0fced71](https://github.com/remi-deher/flux-de-fichiers/commit/0fced71871dd4c079676464631d70e872505bd3e))


### Bug Fixes

* **frontend:** suppression du debordement horizontal sur affichage mobile ([9512bb8](https://github.com/remi-deher/flux-de-fichiers/commit/9512bb8b11bcfd345b4625e052a79bcff1491469))

## [0.6.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.5.0...v0.6.0) (2026-11-11)


### Features

* **frontend:** refonte des flows par type et des bibliotheques sans masque avec acces par noeud ([a75425d](https://github.com/remi-deher/flux-de-fichiers/commit/a75425d6d1cebac0aacfcd7a30e8f3bb625e9cca))
* **frontend:** refonte des flows par type et des bibliotheques sans masque avec acces par noeud ([4787eb5](https://github.com/remi-deher/flux-de-fichiers/commit/4787eb5e3b0ff3cc5f9ce1d8be89df9b598bbb04))

## [0.5.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.4.1...v0.5.0) (2026-11-11)


### Features

* **frontend:** restructuration des pages et navigation inspiree d'encoding-central ([701352c](https://github.com/remi-deher/flux-de-fichiers/commit/701352c57d1706d99752d1971a0f453e9d46590d))
* **frontend:** restructuration des pages et navigation inspiree d'encoding-central ([63feb8e](https://github.com/remi-deher/flux-de-fichiers/commit/63feb8e9e7aaba09aca3b3645a1b754760274a1d))

## [0.4.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.4.0...v0.4.1) (2026-11-11)


### Bug Fixes

* **activity:** dedoublement des selecteurs de filtre pour Playwright ([b39dd9c](https://github.com/remi-deher/flux-de-fichiers/commit/b39dd9ca86d11903343a1a3d396ce692d424b695))
* **ci:** fiabilisation miroir apt pour playwright et compatibilité des filtres de statut ([8135868](https://github.com/remi-deher/flux-de-fichiers/commit/8135868015b6265672e112366442e494b482c7e8))

## [0.4.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.3.0...v0.4.0) (2026-11-11)


### Features

* **flows:** moteur pipeline engine et persistance sqlite des flux ([d5ed350](https://github.com/remi-deher/flux-de-fichiers/commit/d5ed3508e9a615f68338f905ecacd612b78107e5))
* **flows:** pipeline engine avec persistance sqlite, api rest et execution conditionnelle ([eca74c0](https://github.com/remi-deher/flux-de-fichiers/commit/eca74c01a19b80d4ddeca1291d90037d2ba17f02))

## [0.3.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.2.0...v0.3.0) (2026-11-11)


### Features

* architecture conteneur unique style *arr ([#47](https://github.com/remi-deher/flux-de-fichiers/issues/47)/[#50](https://github.com/remi-deher/flux-de-fichiers/issues/50)) ([e89de7f](https://github.com/remi-deher/flux-de-fichiers/commit/e89de7fa397c4395f16477c628d9b581022a68ff))
* architecture conteneur unique style *arr (HTTP 3000, SQLite /config, UI intégrée) ([2f3af7e](https://github.com/remi-deher/flux-de-fichiers/commit/2f3af7e5127943516876e57cd974af4e140078fc))
* **auth:** auto-session web *arr ([#51](https://github.com/remi-deher/flux-de-fichiers/issues/51)) ([076a045](https://github.com/remi-deher/flux-de-fichiers/commit/076a045513d427113a557aee03eabcb6073c1394))
* **auth:** ouvrir automatiquement la session web *arr et supprimer le toast 401 initial ([#51](https://github.com/remi-deher/flux-de-fichiers/issues/51)) ([1b1d04f](https://github.com/remi-deher/flux-de-fichiers/commit/1b1d04faad1fa7b9f20e8d894936b324e3bd0a35))
* cache d'outils, télémétrie et panneau Plugins ([6512dfd](https://github.com/remi-deher/flux-de-fichiers/commit/6512dfd0b3abd34944a61c606225b2fc54843d13))
* **flows:** adr-0009 blocs d'action découplés, webhooks contextuels et progression sse ([321d5f8](https://github.com/remi-deher/flux-de-fichiers/commit/321d5f8b0e37ded30f8c85d7f6749263059a5346))
* **frontend/flows:** modularisation, simulation adr-0008, blocs d'action adr-0009 et sse ([84c3e08](https://github.com/remi-deher/flux-de-fichiers/commit/84c3e084bcee8d8f1659824bec59a3d3e70c1ca8))
* **frontend:** modularisation css/js, simulation tri-sorties adr-0008 et flux sse ([bfeea3d](https://github.com/remi-deher/flux-de-fichiers/commit/bfeea3d506eb324469f2f72340a957901f398f45))
* **frontend:** premiere interface Leptos et delegation des builds a la CI ([c8b0ec4](https://github.com/remi-deher/flux-de-fichiers/commit/c8b0ec411c7cf619498716aa0cb1aa5e4c12ec50))
* gestionnaire de flux no-code type zapier, setup wizard et purge de résilience ([cd3d480](https://github.com/remi-deher/flux-de-fichiers/commit/cd3d4809d459c97d6c82823d6eea0b713d746d15))
* gestionnaire de flux no-code type zapier, setup wizard et purge de résilience ([c8e43fe](https://github.com/remi-deher/flux-de-fichiers/commit/c8e43fef4452a98f2430cddac93ae5dff33fcac4))
* integrate local processing and modularize core ([64d016c](https://github.com/remi-deher/flux-de-fichiers/commit/64d016c5fa0124ae0c27066be043986a93a55570))
* outils distribués, télémétrie, panneau Plugins et CI Playwright ([ff42e32](https://github.com/remi-deher/flux-de-fichiers/commit/ff42e32a2648a0b0a5295b239602251f394c8c22))
* **plugins:** intégration générique de plugins (Sonarr/Radarr) et aiguillage conditionnel tri-sorties (#ADR-0008) ([722e237](https://github.com/remi-deher/flux-de-fichiers/commit/722e2378bd1f78a0091b124c390373c90addc597))
* **plugins:** intégration générique de plugins (Sonarr/Radarr) et bloc conditionnel tri-sorties (#ADR-0008) ([d0325be](https://github.com/remi-deher/flux-de-fichiers/commit/d0325be49e22fee998aefe21e11d7ea7aff40efa))
* **plugins:** intégrer le cache d'outils, le catalogue de plugins et l'ADR 0006 ([7eecb9a](https://github.com/remi-deher/flux-de-fichiers/commit/7eecb9af25c856f500f76a671fe6b96fdfec80fc))
* **plugins:** intégrer le cache d'outils, le catalogue de plugins et l'ADR 0006 ([5a3c5f5](https://github.com/remi-deher/flux-de-fichiers/commit/5a3c5f5680d3fe5c33d7dd6cb848598969883e09))
* session web et poste de pilotage ([#28](https://github.com/remi-deher/flux-de-fichiers/issues/28)/[#29](https://github.com/remi-deher/flux-de-fichiers/issues/29)) ([a759c61](https://github.com/remi-deher/flux-de-fichiers/commit/a759c6179cdc8a3bf069f723a53dc157b2a3080c))
* session web sécurisée et poste de pilotage Leptos ([7d5fc1a](https://github.com/remi-deher/flux-de-fichiers/commit/7d5fc1a963497971e117c50a28dd965016f192dc))


### Bug Fixes

* **ci:** adapter le test fallback SPA, le healthcheck worker et l'accès SQLite des scénarios ([a2fc9b1](https://github.com/remi-deher/flux-de-fichiers/commit/a2fc9b1bf520db845d0a690dfe68ce5a18daef06))
* **ci:** aligner Playwright sur le DOM et épingler la dépendance ([c4def00](https://github.com/remi-deher/flux-de-fichiers/commit/c4def0012f82022276471ee3413dd11f2ed99bbd))
* **ci:** assert le bouton et le flux de téléchargement Playwright ([0bc3440](https://github.com/remi-deher/flux-de-fichiers/commit/0bc3440ee918212f6e6f3e72ceb3d14d6c61a7b2))
* **ci:** synchroniser Cargo.lock et formater le socle *arr ([70f3f89](https://github.com/remi-deher/flux-de-fichiers/commit/70f3f89099691cfb75b969ce66d3fad9d42ee6b6))
* **compose:** définir un healthcheck actif sur worker pour compose --wait ([e9b78b7](https://github.com/remi-deher/flux-de-fichiers/commit/e9b78b7db584abc9b010eda073dda0a3df56a8fb))
* **docker:** copier frontend/static lors du build frontend dans le Dockerfile ([d5a579f](https://github.com/remi-deher/flux-de-fichiers/commit/d5a579fe63459aa6d19647bc0e677986ab36b0c6))
* **docker:** invalider le stub lib.rs lors du build frontend pour générer index.html complet ([65d13a5](https://github.com/remi-deher/flux-de-fichiers/commit/65d13a5bc3f4ebb3efd914d196277ae80805d7ea))
* retirer l'emprunt inutile Clippy sur expires_at ([29d7284](https://github.com/remi-deher/flux-de-fichiers/commit/29d72847e7f8bbaa279869d4c178d3e5160c3d04))
* **test:** adapter playwright au flux sse continu en utilisant domcontentloaded ([4774baf](https://github.com/remi-deher/flux-de-fichiers/commit/4774bafb83747f704a263909ff63ed52e56a025c))

## [0.2.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.1.0...v0.2.0) (2026-11-08)


### Features

* add Dockerized MKVToolNix remux pipeline ([ef70373](https://github.com/remi-deher/flux-de-fichiers/commit/ef7037324ac1e80fb5e77ef408067ad12ac2b1e5))
