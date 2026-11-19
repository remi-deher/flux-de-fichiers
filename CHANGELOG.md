# Changelog

## [0.50.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.49.0...v0.50.0) (2026-11-19)


### Features

* **flows:** outils auxiliaires, fichier d'origine et entrées non média ([7dc0ebe](https://github.com/remi-deher/flux-de-fichiers/commit/7dc0ebe4da42eb263df8ea552780953725e2ea2e))

## [0.49.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.48.0...v0.49.0) (2026-11-19)


### ⚠ BREAKING CHANGES

* **plugins:** les identifiants changent (catalogue 2.0.0 requis) ; les secrets de plugins chiffrés avec l'ancien identifiant ne sont plus lisibles (installation neuve, ADR 0012).

### Features

* **flows:** reprise des anciens types de blocs dans les flux enregistrés ([c554350](https://github.com/remi-deher/flux-de-fichiers/commit/c554350a39c9b98170611c97f089f05c620a3a43))
* **plugins:** identifiants sans préfixe, dates et installation des dépendances ([dadc865](https://github.com/remi-deher/flux-de-fichiers/commit/dadc865f954d1c2bd3f5b923dab67d1fd885a72f))
* **plugins:** reprise automatique des réglages et secrets des plugins renommés ([3178b56](https://github.com/remi-deher/flux-de-fichiers/commit/3178b56cbc49b39fc047a07ae75010dd508c4a68))

## [0.48.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.47.0...v0.48.0) (2026-11-18)


### Features

* **ui:** accessibilité WCAG 2.2 AA, cibles tactiles et éditeur de flux au clavier ([474c3b4](https://github.com/remi-deher/flux-de-fichiers/commit/474c3b42a876016620faeb597dc818a3a247660d))
* **ui:** accessibilité WCAG 2.2 AA, cibles tactiles et éditeur de flux au clavier ([1c6edf7](https://github.com/remi-deher/flux-de-fichiers/commit/1c6edf7b3f561abce8eb03c80da3e052ecf85b56))

## [0.47.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.46.0...v0.47.0) (2026-11-18)


### Features

* **ai:** réponse de l'assistant au fil de l'écriture (empilée sur [#296](https://github.com/remi-deher/flux-de-fichiers/issues/296)) ([f6db212](https://github.com/remi-deher/flux-de-fichiers/commit/f6db212a0b647cb9fd76fbcc51e5dfc9ee94abcf))

## [0.46.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.45.1...v0.46.0) (2026-11-18)


### Features

* **ai:** conversation en direct (runs SSE) et contexte automatique ([860a387](https://github.com/remi-deher/flux-de-fichiers/commit/860a38797110bc8c2803993bbab62bcafcc38827))
* **ai:** suivre la conversation de l'assistant en direct et lui joindre l'écran ouvert ([9abbea3](https://github.com/remi-deher/flux-de-fichiers/commit/9abbea336e2753011a91d9d64b5b7eeac4f583da))


### Bug Fixes

* **ai:** lister les modèles ChatGPT avec une version de client Codex ([9a7e3ec](https://github.com/remi-deher/flux-de-fichiers/commit/9a7e3ecbb8640bcea4dcb7ac145e01bfaf5ef698))
* **ai:** lister les modèles ChatGPT avec une version de client Codex et afficher la raison d'une liste vide ([89e35ba](https://github.com/remi-deher/flux-de-fichiers/commit/89e35bacfc4ca2722edd7fe4932149a12d96d3a0))

## [0.45.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.45.0...v0.45.1) (2026-11-18)


### Bug Fixes

* **version:** afficher la version de la release et lister les modèles IA proposés ([cdb0cdd](https://github.com/remi-deher/flux-de-fichiers/commit/cdb0cdd6d027b5da0a46636b247dd552769f9d98))
* **version:** afficher la version de la release et lister les modèles IA proposés ([d2296d1](https://github.com/remi-deher/flux-de-fichiers/commit/d2296d182dc195fe7a2493e55bc96c2362d5df0a))

## [0.45.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.44.0...v0.45.0) (2026-11-18)


### Features

* **logs:** journal applicatif dans Activité et modèle ChatGPT automatique ([46e7401](https://github.com/remi-deher/flux-de-fichiers/commit/46e7401a3ce4cdc20440c2d33acb5bc0493deb41))
* **logs:** journal applicatif dans Activité et modèle ChatGPT automatique ([537f7bb](https://github.com/remi-deher/flux-de-fichiers/commit/537f7bb43e94e309405a9a4dd2ecb0876741fb25))

## [0.44.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.43.0...v0.44.0) (2026-11-18)


### Features

* **ai:** connexion OAuth ChatGPT pour l'assistant intégré ([3378a1b](https://github.com/remi-deher/flux-de-fichiers/commit/3378a1b3b554e3abb37e75d91dd6e124c07f3f62))
* **ai:** connexion OAuth ChatGPT pour l'assistant intégré ([430ab2b](https://github.com/remi-deher/flux-de-fichiers/commit/430ab2bcd457bb063f237a7bfd427360e88ae5c1))

## [0.43.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.42.0...v0.43.0) (2026-11-18)


### Features

* protocole MCP enrichi, simulation dry-run et assistant IA web ([07dbbde](https://github.com/remi-deher/flux-de-fichiers/commit/07dbbde2fcb2fb95e6750bad22cc161911079f3b))
* protocole MCP enrichi, simulation dry-run et assistant IA web ([a508e93](https://github.com/remi-deher/flux-de-fichiers/commit/a508e93f868869e28e29d7f46c96bcb9c42a5f9e))


### Bug Fixes

* **ai:** retirer un import inutilisé hors tests ([3fa07e8](https://github.com/remi-deher/flux-de-fichiers/commit/3fa07e890b3210dc5c0d47a8a2ed688c6c84d790))
* **client:** simplifier l'appel use_context dans la barre supérieure ([3d01ac2](https://github.com/remi-deher/flux-de-fichiers/commit/3d01ac21a625d3b11d7a2c480a7c628990d5a977))

## [0.42.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.41.0...v0.42.0) (2026-11-18)


### Features

* approval enrollment, library automation, job scheduling, sqlx offline and mcp audit ([a1f5316](https://github.com/remi-deher/flux-de-fichiers/commit/a1f5316050dd5045331624c8df96ee6027375ee0))
* enrôlement par approbation, automatisation, SQLx hors-ligne et audit MCP ([d2b6da9](https://github.com/remi-deher/flux-de-fichiers/commit/d2b6da95e03dbe46c6add7025cdffa896d92d91f))


### Bug Fixes

* collapse nested condition in worker schedule validation ([c15d4d7](https://github.com/remi-deher/flux-de-fichiers/commit/c15d4d7bc2084c111399af048071cde3680b2c4b))
* simplifier le rendu conditionnel dans workers ([d5e4ed4](https://github.com/remi-deher/flux-de-fichiers/commit/d5e4ed48b2a464065d4b3c843a556d6252525ac8))
* simplify conditional header actions in workers view ([9f102c9](https://github.com/remi-deher/flux-de-fichiers/commit/9f102c9793ad76e9d9b345d24a33ff190d5585fd))

## [0.41.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.40.0...v0.41.0) (2026-11-18)


### Features

* **ui:** organise la page Paramètres en onglets horizontaux ([bc3e8ed](https://github.com/remi-deher/flux-de-fichiers/commit/bc3e8ed2cb309a6e9c8844ce8b33f451289055e2))
* **ui:** organise la page Paramètres en onglets horizontaux ([1d70fb2](https://github.com/remi-deher/flux-de-fichiers/commit/1d70fb241ddc84324def21f88d904dc6030eb84f))

## [0.40.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.39.0...v0.40.0) (2026-11-18)


### Features

* **ui:** refonte du drawer style FileFlows, fiche tâche à onglets et registre des flows ([14cb940](https://github.com/remi-deher/flux-de-fichiers/commit/14cb940149d21ee9ba503aff963758f4eb088c71))
* **ui:** refonte du drawer style FileFlows, fiche tâche à onglets et registre des flows ([d48ea67](https://github.com/remi-deher/flux-de-fichiers/commit/d48ea671baa24e50723b774338b26c1da0713c66))


### Bug Fixes

* **ui:** supprime un emprunt superflu pour clippy ([72503df](https://github.com/remi-deher/flux-de-fichiers/commit/72503dfe53b50fe2c51306f13870411a9d27cb5f))

## [0.39.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.38.1...v0.39.0) (2026-11-18)


### Features

* **libraries:** sélecteur d'arborescence de dossiers/médias et édition en ligne des workers ([de1e9b4](https://github.com/remi-deher/flux-de-fichiers/commit/de1e9b47d01a33aea64763fb6ec26569aed61213))
* **libraries:** sélecteur d'arborescence de dossiers/médias et édition en ligne des workers ([d15a78c](https://github.com/remi-deher/flux-de-fichiers/commit/d15a78c95fb2789d4cfe1cb6cb048b769f32ab02))

## [0.38.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.38.0...v0.38.1) (2026-11-18)


### Bug Fixes

* **flows:** synchronisation SVG pan/zoom, tri anti-croisement et cartes compactes ([fd2eeb3](https://github.com/remi-deher/flux-de-fichiers/commit/fd2eeb331940fb9d85a9dab063e44fa4fc81a519))
* **flows:** synchronisation SVG pan/zoom, tri anti-croisement et cartes compactes ([b4d7758](https://github.com/remi-deher/flux-de-fichiers/commit/b4d77588ca5a6e4565d45f386aba9183f8a7368a))

## [0.38.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.37.0...v0.38.0) (2026-11-18)


### Features

* **flows:** refonte studio spatial à îlots flottants et cartes nodales modulaires ([d6ba253](https://github.com/remi-deher/flux-de-fichiers/commit/d6ba25337275db83f267135eb35f2a43e48967a5))
* **flows:** refonte studio spatial à îlots flottants et cartes nodales modulaires ([d04cca3](https://github.com/remi-deher/flux-de-fichiers/commit/d04cca3bd6f8c1dd424c2fb4719b9df5b97c434e))

## [0.37.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.36.0...v0.37.0) (2026-11-18)


### Features

* **ui:** render plugin blocks as flow palette cards ([2482363](https://github.com/remi-deher/flux-de-fichiers/commit/24823636d3d4f2604d9adc0b354b78a831facc5a))
* **ui:** render plugin blocks as flow palette cards ([5676e9d](https://github.com/remi-deher/flux-de-fichiers/commit/5676e9d0920254df1f768ead7d51f755ea381ac7))

## [0.36.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.35.0...v0.36.0) (2026-11-18)


### Features

* add selectable API key scopes and simplify technical UI/MCP nomenclature ([ba5dd1d](https://github.com/remi-deher/flux-de-fichiers/commit/ba5dd1d90257c8ec29bab195aa89df1bf9c4f72f))
* add selectable API key scopes and simplify technical UI/MCP nomenclature ([f7f63fc](https://github.com/remi-deher/flux-de-fichiers/commit/f7f63fc3b5583af6f432879a28e5999b1e46502a))

## [0.35.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.34.1...v0.35.0) (2026-11-16)


### Features

* add native MCP server, mcp API key role, and library/flow/job API endpoints ([c67291b](https://github.com/remi-deher/flux-de-fichiers/commit/c67291ba9bbb4a475e2ba9ec3ff48c56c01bec4c))
* add native MCP server, mcp API key role, and library/flow/job API endpoints ([23ff35c](https://github.com/remi-deher/flux-de-fichiers/commit/23ff35c14227e82b8d784cd04b34bc1b455e2f35))


### Bug Fixes

* **docker:** copy docs/openapi.yaml into build stage ([06a134b](https://github.com/remi-deher/flux-de-fichiers/commit/06a134bf28ecbedb30f064708963974a9be34bc5))
* **docker:** copy docs/openapi.yaml into build stage ([4b7d730](https://github.com/remi-deher/flux-de-fichiers/commit/4b7d7305a456ea6b9008ffbda79f754efaae8432))
* **docker:** include docs/openapi.yaml in docker build context ([fe9926d](https://github.com/remi-deher/flux-de-fichiers/commit/fe9926dd2bc46601472bc5fe6eba4301b9bf5317))
* **docker:** include docs/openapi.yaml in docker build context ([7bd0df4](https://github.com/remi-deher/flux-de-fichiers/commit/7bd0df485ce17b4cf1c4b80cb53596af21f57990))

## [0.34.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.34.0...v0.34.1) (2026-11-16)


### Bug Fixes

* **clippy:** avoid unit_arg in tool_detail empty branch ([79b3809](https://github.com/remi-deher/flux-de-fichiers/commit/79b380964a5c62176f91b9bf3868b2e87fa49bc8))
* show human names in Extensions drawers and guard non-installable tools ([2d65bd9](https://github.com/remi-deher/flux-de-fichiers/commit/2d65bd94f5a9f02c804517b5650bbd6b42cec00e))
* show human names in Extensions drawers and guard non-installable tools ([2af396f](https://github.com/remi-deher/flux-de-fichiers/commit/2af396fb770372d96981a6f49c1b604f46afb454))

## [0.34.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.33.0...v0.34.0) (2026-11-16)


### Features

* **flows:** support du bloc unique Script et routage conditionnel ([c1268ef](https://github.com/remi-deher/flux-de-fichiers/commit/c1268effb406a53508cbedf9074027a5943bc9da))
* **flows:** support unified script block with library/inline modes and conditional routing ([98d670a](https://github.com/remi-deher/flux-de-fichiers/commit/98d670ac33c451938b676308a9782e123ba9302f))

## [0.33.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.32.0...v0.33.0) (2026-11-16)


### Features

* **libraries:** finaliser et pérenniser le catalogue local ([7a21e44](https://github.com/remi-deher/flux-de-fichiers/commit/7a21e442abacfd8acc1dd4b0dd990b1263fc4200))
* **libraries:** harden local catalog, UI drawers, engine tests and docs ([c356137](https://github.com/remi-deher/flux-de-fichiers/commit/c35613709accf337afd48dddb107df00c728c0f6))


### Bug Fixes

* **client:** resolve clippy lints in libraries drawers ([b61980f](https://github.com/remi-deher/flux-de-fichiers/commit/b61980fb8f3359f69112767c25571607040a9ba3))

## [0.32.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.31.0...v0.32.0) (2026-11-16)


### Features

* **ui:** move Librairies create actions to topbar and rename Extensions ([fddb9ee](https://github.com/remi-deher/flux-de-fichiers/commit/fddb9ee9b7db375727c60fa6e49173efbda9ab60))
* **ui:** move Librairies create actions to topbar and rename Extensions ([3813bd1](https://github.com/remi-deher/flux-de-fichiers/commit/3813bd103dead1af740d27764b3a457f777ea611))

## [0.31.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.30.0...v0.31.0) (2026-11-16)


### Features

* **ui,plugins:** tiroirs à onglets, runtime script stdio et librairie locale ([c95bc66](https://github.com/remi-deher/flux-de-fichiers/commit/c95bc66555f069b607b347f32a1eaf21a30cd948))
* **ui,plugins:** tiroirs à onglets, runtime script stdio et librairie locale ([125ef96](https://github.com/remi-deher/flux-de-fichiers/commit/125ef96b51dc044fa013916d41a27f346706b999))


### Bug Fixes

* **clippy:** use as_chunks in base64 and avoid expect_err in test ([6bd4ca2](https://github.com/remi-deher/flux-de-fichiers/commit/6bd4ca26e25dad58220c4ca01da275295c2407ff))

## [0.30.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.29.0...v0.30.0) (2026-11-16)


### Features

* **plugins:** releases immuables, SHA-256 complet et index.json à la racine ([d1697fc](https://github.com/remi-deher/flux-de-fichiers/commit/d1697fc7cbfb557d9ddf7e4e4ea8291cc72df784))
* **plugins:** releases immuables, SHA-256 complet et index.json à la racine ([a973500](https://github.com/remi-deher/flux-de-fichiers/commit/a973500dd563057e9b0e3f817315af24e6d2d1cd))


### Bug Fixes

* **plugins:** supprimer expect() dans le test de sync pour clippy ([b9d1165](https://github.com/remi-deher/flux-de-fichiers/commit/b9d1165da4ff2b613639a7a7d8827be4b2d6c89a))

## [0.29.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.28.0...v0.29.0) (2026-11-16)


### Features

* moteur et sondes agnostiques, conditions génériques et découplage UI ([62568fa](https://github.com/remi-deher/flux-de-fichiers/commit/62568fa9a56910f14b966776f78d31746a549de2))
* moteur et sondes agnostiques, conditions génériques et découplage UI ([d7c39f0](https://github.com/remi-deher/flux-de-fichiers/commit/d7c39f0414bcc1b940ebf92d4906dae1c52bb67b))


### Bug Fixes

* **clippy:** corriger large_enum_variant et as_chunks ([40140fe](https://github.com/remi-deher/flux-de-fichiers/commit/40140fe240888612f170283c76072d2f52abdc61))

## [0.28.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.27.0...v0.28.0) (2026-11-15)


### Features

* durcissement backend et fin du backlog interface wasm ([b3cd93a](https://github.com/remi-deher/flux-de-fichiers/commit/b3cd93a20aed1521a3cbcc470775f0fed7b31d39))
* durcissement backend et fin du backlog interface wasm ([df101ac](https://github.com/remi-deher/flux-de-fichiers/commit/df101ac55ad27434b110ccaa5af59e3e1cfe62a0))
* moteur déclaratif par contrats et écrans WASM ([a703d6e](https://github.com/remi-deher/flux-de-fichiers/commit/a703d6e3c2278a5c17c17e55b37f615253917f4f))
* moteur declaratif par contrats et ecrans wasm associes ([e9afafb](https://github.com/remi-deher/flux-de-fichiers/commit/e9afafb3be9e3450293362a700be71e9d6628b6c))


### Bug Fixes

* **clippy:** corrections lints runtime settings et client wasm ([940d017](https://github.com/remi-deher/flux-de-fichiers/commit/940d0170311ae6e540f91f6b4fa4293bb9633289))
* **clippy:** corrections lints sur moteur declaratif ([6cc341d](https://github.com/remi-deher/flux-de-fichiers/commit/6cc341d87c4daff5d3626ffc0d1eb5302ff51970))
* **clippy:** utilisation de sort_by_key sur la liste des bibliotheques ([4a0ed5f](https://github.com/remi-deher/flux-de-fichiers/commit/4a0ed5fbeca7a48772c6907e06a7b758aeda3c6f))

## [0.27.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.26.0...v0.27.0) (2026-11-15)


### Features

* 10 ameliorations backend, plugins et securite ([ec959e6](https://github.com/remi-deher/flux-de-fichiers/commit/ec959e6a8f092526f06a0fbdf9de4822b655e78b))
* 10 ameliorations backend, plugins, securite et observabilite ([a91e111](https://github.com/remi-deher/flux-de-fichiers/commit/a91e1118e45f3cbd7788badf8ffac558b81105e5))


### Bug Fixes

* **clippy:** reutiliser FlowProgressCallback dans job_runner ([b45ba96](https://github.com/remi-deher/flux-de-fichiers/commit/b45ba961acb50b334fbdf5f4d9bea3091e377791))

## [0.26.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.25.0...v0.26.0) (2026-11-15)


### Features

* **flows:** améliorer le drag-and-drop sur grille et synchroniser les liaisons ([02883cc](https://github.com/remi-deher/flux-de-fichiers/commit/02883ccbf481a8bc4846b7449694c096056d35f0))
* **flows:** améliorer le drag-and-drop sur grille et synchroniser les liaisons ([7ea26a4](https://github.com/remi-deher/flux-de-fichiers/commit/7ea26a4f397dcdfa4a832a2f9fe2140c27744337))

## [0.25.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.24.0...v0.25.0) (2026-11-15)


### Features

* **flows:** zéro bloc embarqué dans le moteur ([c68c00f](https://github.com/remi-deher/flux-de-fichiers/commit/c68c00fe807bafd39aa3501dd918355e9ce5635b))
* **flows:** zéro bloc embarqué dans le moteur ([65b6c52](https://github.com/remi-deher/flux-de-fichiers/commit/65b6c52cb3342778969e13e9fb6f4cd40089fa13))

## [0.24.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.23.0...v0.24.0) (2026-11-15)


### Features

* **flows:** ergonomie low-code (focus, canvas infini, quick-add, multi-sélection) ([6b046e5](https://github.com/remi-deher/flux-de-fichiers/commit/6b046e505a6a901285ec4d2293e7bcabd2cd1fd5))
* **flows:** ergonomie low-code de l'éditeur de flux ([69bff82](https://github.com/remi-deher/flux-de-fichiers/commit/69bff821fca399897df8bd87d4f0930501a0d011))

## [0.23.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.22.0...v0.23.0) (2026-11-15)


### Features

* **flows:** refondre l'éditeur de flux en atelier nodal interactif ([c12b140](https://github.com/remi-deher/flux-de-fichiers/commit/c12b140b4261139be6c126520f78e1589f66a817))
* **flows:** refondre l'éditeur de flux en atelier nodal interactif ([8e78cb8](https://github.com/remi-deher/flux-de-fichiers/commit/8e78cb873ee09669d0ca2b705b489dd8afa6c602))


### Bug Fixes

* **flows:** corriger deux comparaisons Clippy dans les tests DnD ([912cf08](https://github.com/remi-deher/flux-de-fichiers/commit/912cf08f3e1c10df5ad6621e7b57c6e4ff24eb61))

## [0.22.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.21.1...v0.22.0) (2026-11-15)


### Features

* **ui:** unifie le système de design, les grilles et la barre supérieure ([8e14e83](https://github.com/remi-deher/flux-de-fichiers/commit/8e14e83209238741b9e06e69b5f1068a4dc4cf17))
* **ui:** unifie le système de design, les grilles et la barre supérieure ([eb91ab9](https://github.com/remi-deher/flux-de-fichiers/commit/eb91ab9b70186b67b759e275aebdd9dfe78308c8))


### Bug Fixes

* **ui:** retire les clones inutiles signalés par clippy ([461bc7c](https://github.com/remi-deher/flux-de-fichiers/commit/461bc7c1eefea7ab8de12ac5449022df650ca390))

## [0.21.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.21.0...v0.21.1) (2026-11-15)


### Bug Fixes

* **ui:** corriger l'accolade CSS de page-transition-region et l'alignement topbar ([02d8320](https://github.com/remi-deher/flux-de-fichiers/commit/02d83207b0e07c99b258bed75ae20ca4533f1151))
* **ui:** refondre workers, plugins et parametres et epurer la navigation ([b415839](https://github.com/remi-deher/flux-de-fichiers/commit/b415839746b2b6e30339814f9cdab16f47573335))
* **ui:** refondre workers, plugins et paramètres et épurer la navigation ([b88725e](https://github.com/remi-deher/flux-de-fichiers/commit/b88725e476889aa6fb06f6d174ab42a6060167d9))

## [0.21.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.20.0...v0.21.0) (2026-11-15)


### Features

* **ui:** refondre le shell et l'ensemble des écrans en Rust ([26a0412](https://github.com/remi-deher/flux-de-fichiers/commit/26a04127ac4d0ae7f30ab64db82df97095666a80))
* **ui:** refondre le shell et l'ensemble des écrans en Rust ([76847db](https://github.com/remi-deher/flux-de-fichiers/commit/76847db97e1e4a8553605b78f8770621d6b3d6d1))

## [0.20.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.19.0...v0.20.0) (2026-11-15)


### Features

* **ui:** moderniser l'interface 100 % Rust ([9a34d7a](https://github.com/remi-deher/flux-de-fichiers/commit/9a34d7a6225c04ec9bdd87b3551ec6481c4ca0ec))
* **ui:** moderniser le thème, la palette de commandes et les composants réactifs ([7cf5f98](https://github.com/remi-deher/flux-de-fichiers/commit/7cf5f988a673db0e58e4e350e71ac7fb2a9b9156))

## [0.19.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.18.0...v0.19.0) (2026-11-14)


### Features

* **client:** actions dans la recherche, retrait des subnavs ([ceb9dec](https://github.com/remi-deher/flux-de-fichiers/commit/ceb9dec2d98e251d39b6bb047cf35a0b2d8c8935))
* **client:** actions dans la recherche, retrait des subnavs et de la création de tâche ([110dd77](https://github.com/remi-deher/flux-de-fichiers/commit/110dd77fe545a7f752fe2ddb7daf89aaf83c6818))

## [0.18.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.17.0...v0.18.0) (2026-11-14)


### Features

* **client:** tableau à onglets attachés, subnav compacte, action dans la recherche ([23b84c9](https://github.com/remi-deher/flux-de-fichiers/commit/23b84c9dd99c03f0c7db98ec9b961aa26542ed79))
* **client:** tableau à onglets attachés, subnav compacte, action dans la recherche ([c0813c6](https://github.com/remi-deher/flux-de-fichiers/commit/c0813c68097243c3ee6307292f2b5f97bb39ea11))

## [0.17.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.16.0...v0.17.0) (2026-11-14)


### Features

* **plugins:** sections du catalogue et propriétaire des blocs ([c7ea6e8](https://github.com/remi-deher/flux-de-fichiers/commit/c7ea6e8372b04108274d1f7f04dcb991e644a2cd))
* **plugins:** sections du catalogue et propriétaire des blocs ([17b9b17](https://github.com/remi-deher/flux-de-fichiers/commit/17b9b17f3782aaaf5e131d59decf2164c97af6db))

## [0.16.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.15.0...v0.16.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* **client:** l'interface exige JavaScript et WebAssembly ; le volume qui sert l'interface doit recevoir index.html et le dossier pkg/.

### Features

* **acces:** mot de passe oublié par courriel SMTP ([0c0c74f](https://github.com/remi-deher/flux-de-fichiers/commit/0c0c74ffd8b755d61150136471f6697dbee5cc58))
* **acces:** mot de passe oublié par courriel SMTP (ADR 0015) ([52e1cf6](https://github.com/remi-deher/flux-de-fichiers/commit/52e1cf6aafb48926a70320863f12d5a703d5e028))
* **accounts:** réinitialiser un mot de passe oublié en ligne de commande ([b19510f](https://github.com/remi-deher/flux-de-fichiers/commit/b19510f55a84e1b05003c22d8f03c7e760adef97))
* **accounts:** rôle lecteur et gestion des comptes (ADR 0011) ([89e594c](https://github.com/remi-deher/flux-de-fichiers/commit/89e594c7451b4bf3dd557350e62c3f0e9b1dea1a))
* **accounts:** rôle lecteur et gestion des comptes (ADR 0011) ([b4d0c33](https://github.com/remi-deher/flux-de-fichiers/commit/b4d0c337ad72b977c5a6c3a2b3dd60535308a078))
* **auth:** installation guidée, comptes et clés d'API (ADR 0011, phase 1) ([55f0b9c](https://github.com/remi-deher/flux-de-fichiers/commit/55f0b9cc876b577c98f5706da5b5c594061a6b71))
* client Leptos WASM, flux versionnés, relance manuelle et vocabulaire central / worker ([34f365e](https://github.com/remi-deher/flux-de-fichiers/commit/34f365e9af28c1ea04b9fbc0a47bc11d52c9814b))
* **client:** choisir le worker à la création d'une tâche ([dba9497](https://github.com/remi-deher/flux-de-fichiers/commit/dba94970b514a7442298ea98e4dcaab524f9296a))
* **client:** choisir le worker à la création d'une tâche ([7fc2898](https://github.com/remi-deher/flux-de-fichiers/commit/7fc289879d2bdaec1d0268210de3a23e6ae1450d))
* **client:** client Leptos WASM sous /app et écran Tâches (ADR 0014) ([758a42f](https://github.com/remi-deher/flux-de-fichiers/commit/758a42fb7a35650c3faa1ec4e6367f7a1f8ae50e))
* **client:** feuille de filtres de la barre du haut ([ff291be](https://github.com/remi-deher/flux-de-fichiers/commit/ff291be7df116200d2bee6035d5b796ed50bfcb3))
* **client:** feuille de filtres de la barre du haut ([637f642](https://github.com/remi-deher/flux-de-fichiers/commit/637f6427eac2d185ad41b44308eaae5401f1877d))
* **client:** flux, bibliothèques et workers dans le client WASM (ADR 0014) ([383e86c](https://github.com/remi-deher/flux-de-fichiers/commit/383e86ccc78c15999a99a4b96281e02eace09b0f))
* **client:** glisser-déposer dans l'éditeur de flux et garde de sortie ([d82336b](https://github.com/remi-deher/flux-de-fichiers/commit/d82336bedd7fccc8a0051bf3bf8eec6f13323797))
* **client:** glisser-déposer dans l'éditeur de flux et garde de sortie ([7196615](https://github.com/remi-deher/flux-de-fichiers/commit/719661535acc77710f9cae18cbe26c0d38d4c2fd))
* **client:** navigation mobile en tiroir ([8df8f3b](https://github.com/remi-deher/flux-de-fichiers/commit/8df8f3b2a3e05fd012b995f65e7adae61658e698))
* **client:** navigation mobile en tiroir ([fbe7c57](https://github.com/remi-deher/flux-de-fichiers/commit/fbe7c57c1f7cfc3da0108bb171dee8763b683a1c))
* **client:** pagination des tâches au-delà de 200 ([539103c](https://github.com/remi-deher/flux-de-fichiers/commit/539103c745ecd9de6075ddf9745f69b2032e2468))
* **client:** pagination des tâches au-delà de 200 ([bd2a9ca](https://github.com/remi-deher/flux-de-fichiers/commit/bd2a9ca320e64ad0a76605ec38b28963f2a65026))
* **client:** Paramètres, Stockage retiré et Mises à jour fusionné dans Système ([d6bec15](https://github.com/remi-deher/flux-de-fichiers/commit/d6bec1585bae8b1c7db7fe3f86291779433050e6))
* **client:** Paramètres, Stockage retiré, Mises à jour fusionné dans Système ([a78cf7e](https://github.com/remi-deher/flux-de-fichiers/commit/a78cf7ed122626b4a4d2ef19b7e467edfff9e04b))
* **client:** recherche globale de la barre du haut ([075518d](https://github.com/remi-deher/flux-de-fichiers/commit/075518dab2316fad6daafffbda91625ed2950c0d))
* **client:** recherche globale de la barre du haut ([d2ff20d](https://github.com/remi-deher/flux-de-fichiers/commit/d2ff20d0c847b6775082d8c3f6a245f74ad99584))
* **client:** session expirée, retour automatique à la connexion ([b1efec5](https://github.com/remi-deher/flux-de-fichiers/commit/b1efec54d8355f172876281293ab485c27e4321f))
* **client:** session expirée, retour automatique à la connexion ([176e4c7](https://github.com/remi-deher/flux-de-fichiers/commit/176e4c7bd276826aea08741d771e1ac9e26658d3))
* **client:** toute l'interface dans le client WASM, bascule de / (ADR 0014) ([2bcd32f](https://github.com/remi-deher/flux-de-fichiers/commit/2bcd32f590bb7822c7df3a1165754220d797c433))
* **client:** widgets personnalisables de la vue d'ensemble ([800f1bc](https://github.com/remi-deher/flux-de-fichiers/commit/800f1bceb906724ec8592c90d79b9b37496dea27))
* **client:** widgets personnalisables de la vue d'ensemble ([27b1350](https://github.com/remi-deher/flux-de-fichiers/commit/27b13502d04e510ff5a8bc89487e27837eae98ba))
* **flows:** consulter et restaurer les versions d'un flux ([67cb9cc](https://github.com/remi-deher/flux-de-fichiers/commit/67cb9ccc4d4545d033c7b949797f47176ac75c90))
* **flows:** consulter et restaurer les versions d'un flux ([50ad013](https://github.com/remi-deher/flux-de-fichiers/commit/50ad01375787e99109e50074910c24d9a8774558))
* **flows:** exécuter block.video.remux-mp4 via MP4Box ([9a1712e](https://github.com/remi-deher/flux-de-fichiers/commit/9a1712e56279d3256907c83e704ab59c9a5981f2))
* **flows:** exécuter block.video.remux-mp4 via MP4Box ([c3f255b](https://github.com/remi-deher/flux-de-fichiers/commit/c3f255b81e094b853589e7c462594110106c8bb8))
* **flows:** le bail ne porte que les secrets *arr* utilisés par le flux figé ([735ecf7](https://github.com/remi-deher/flux-de-fichiers/commit/735ecf77626cd885e61b5a487127cb5ad2c217c1))
* **flows:** limiter les secrets *arr* du bail aux blocs du flux figé ([419c222](https://github.com/remi-deher/flux-de-fichiers/commit/419c2223aefd7de051c4dd35a72574f970c71b11))
* **flows:** palette et exécution des blocs d'encodage plugins ([5095a55](https://github.com/remi-deher/flux-de-fichiers/commit/5095a55a23fc6f537e36055c67c77990134f1835))
* **flows:** palette et exécution des blocs d'encodage plugins ([b1e898c](https://github.com/remi-deher/flux-de-fichiers/commit/b1e898cb4a39e68d34a725474468035307c81f8c))
* **flows:** repli automatique GPU vers CPU pour l'encodage ([62fe68b](https://github.com/remi-deher/flux-de-fichiers/commit/62fe68b060dd4d4f142a9cb1fdb726df7cf82370))
* **flows:** repli automatique GPU vers CPU quand l'encodeur matériel échoue ([c349f7b](https://github.com/remi-deher/flux-de-fichiers/commit/c349f7bea70af882ddc8c545f6e706b92c872e5f))
* flux figé sur workers distants, progression SSE, blocs d'encodage plugins ([0c6de4f](https://github.com/remi-deher/flux-de-fichiers/commit/0c6de4f0a632181b0df15246668d225e081433a7))
* **frontend:** modification des utilisateurs dans un tiroir, déconnexion discrète ([763d444](https://github.com/remi-deher/flux-de-fichiers/commit/763d4443ced6eea46d8da9a6e174d1b05725cbc2))
* **frontend:** modification des utilisateurs dans un tiroir, déconnexion discrète dans la barre latérale (ADR 0011) ([11d5c51](https://github.com/remi-deher/flux-de-fichiers/commit/11d5c5198ffe3029130d5d66b0ba8dacbc1448ab))
* **frontend:** page Paramètres en sections, compte et thème dans la barre latérale ([28a81f7](https://github.com/remi-deher/flux-de-fichiers/commit/28a81f724ea4b290bf85cd63e6b92d07ad65e803))
* **frontend:** page Paramètres en sections, compte et thème dans la barre latérale (ADR 0011) ([da1a9f2](https://github.com/remi-deher/flux-de-fichiers/commit/da1a9f2a6ea36e72f1a79a69805d90be5de3ad55))
* **nodes:** enrôlement des workers et choix du rôle (ADR 0011, phase 2) ([02e1547](https://github.com/remi-deher/flux-de-fichiers/commit/02e1547563469a28ea5c0f97d33d31f4daa28bda))
* **nodes:** enrôlement des workers, vues et connexion déléguée (ADR 0011, phases 2 à 4) ([9abaed8](https://github.com/remi-deher/flux-de-fichiers/commit/9abaed8f822d293e88ac5ff4cca4c9882cd40ff9))
* **nodes:** vues du worker, connexion déléguée et tableau de bord de nœud (ADR 0011, phases 3 et 4) ([7030aff](https://github.com/remi-deher/flux-de-fichiers/commit/7030aff8c68a6004d013f45300ba7681db316860))
* **parametres:** onglet Mises à jour ([c3b05fb](https://github.com/remi-deher/flux-de-fichiers/commit/c3b05fb3df86add0d59292fa8c6bebe7245f97c9))
* **parametres:** onglet Mises à jour ([1469721](https://github.com/remi-deher/flux-de-fichiers/commit/1469721aff9ec218d60b5ff08501dc1a6e8876d9))
* **parametres:** onglets Réseau et Stockage ([4984279](https://github.com/remi-deher/flux-de-fichiers/commit/4984279a43fda5e420184098bb89b9dd2465566b))
* **parametres:** onglets Réseau et Stockage ([46055fb](https://github.com/remi-deher/flux-de-fichiers/commit/46055fb1baa0c50756761066bec59ad7e34a16eb))
* relance manuelle, flows versionnés et vocabulaire central / worker (ADR 0012, 0013) ([7fb0bba](https://github.com/remi-deher/flux-de-fichiers/commit/7fb0bba312c8881bf81e18c0c4c6431e94097862))
* **reseau:** connectivité des workers et diagnostic (lot A) ([f7cc768](https://github.com/remi-deher/flux-de-fichiers/commit/f7cc7689264b1294a4117a583ee85a9885773b6f))
* **reseau:** connectivité des workers, diagnostic et carte Configuration (lot A) ([14017f1](https://github.com/remi-deher/flux-de-fichiers/commit/14017f1366e7590528bd535baf643a55d5e09d91))
* **secrets:** rotation de la clé de chiffrement en ligne de commande (ADR 0011) ([a520b54](https://github.com/remi-deher/flux-de-fichiers/commit/a520b54e1942b6fa1e76016691176c4d07af9ed3))
* **secrets:** rotation de la clé de chiffrement et mot de passe oublié (ADR 0011) ([2601c7c](https://github.com/remi-deher/flux-de-fichiers/commit/2601c7cbb61a03cf4492c81ae9bb4ff4794049fe))
* **vue-ensemble:** télémétrie matérielle du central ([1a96eb2](https://github.com/remi-deher/flux-de-fichiers/commit/1a96eb29c329a7c720e49ff7c5cf005e7036e3c7))
* **vue-ensemble:** télémétrie matérielle du central ([4263b7a](https://github.com/remi-deher/flux-de-fichiers/commit/4263b7af1dff94230d390110d569c11b32ee7043))
* **worker:** exécuter le flux figé sur les workers distants ([e839f62](https://github.com/remi-deher/flux-de-fichiers/commit/e839f6275fcba5c9188896b9a73ea59c9930286f))
* **worker:** remonter la progression SSE depuis les workers distants ([d1d8ec5](https://github.com/remi-deher/flux-de-fichiers/commit/d1d8ec52cf7964e3e005b2f17935c11c5078c0de))
* **worker:** remonter la progression SSE depuis les workers distants ([1481f8c](https://github.com/remi-deher/flux-de-fichiers/commit/1481f8c60fad79b0d1aefa5375ce78dfa55c60c1))


### Bug Fixes

* **acces:** recherche de l'adresse e-mail insensible à la casse ([a0d7e05](https://github.com/remi-deher/flux-de-fichiers/commit/a0d7e055b1cb405015c8feb3def520f52e9ded08))
* **acces:** recherche de l'adresse e-mail insensible à la casse ([fd7fc4d](https://github.com/remi-deher/flux-de-fichiers/commit/fd7fc4d4e15efd537169c8df0c5f6cc2b6afbae8))
* **ci:** copier le crate shared dans l'image et corriger la conversion d'erreur du rôle de nœud ([b19510f](https://github.com/remi-deher/flux-de-fichiers/commit/b19510f55a84e1b05003c22d8f03c7e760adef97))
* **ci:** reconstruire l'image quand client/ ou shared/ change ([d89ae35](https://github.com/remi-deher/flux-de-fichiers/commit/d89ae35c26fc1e165f1a3c6f9adbbf8d084d7e81))
* **ci:** reconstruire l'image quand client/ ou shared/ change ; interface sans cache périmé ([407ec48](https://github.com/remi-deher/flux-de-fichiers/commit/407ec485284b5df4be7dfa492cbc635e494f5282))
* **client:** retrouver le gabarit de l'interface (barre latérale, tiroirs, registres) ([24aaadc](https://github.com/remi-deher/flux-de-fichiers/commit/24aaadc4aebe6197b56e8e43ef4cb98f583be510))
* **client:** retrouver le gabarit de l'interface (barre latérale, tiroirs, registres) ([eccf9cb](https://github.com/remi-deher/flux-de-fichiers/commit/eccf9cbaa8e35a4009b3feb07fc9d9645170480f))
* **client:** test du déplacement de carte dans l'éditeur ([1383a16](https://github.com/remi-deher/flux-de-fichiers/commit/1383a163a12e0df0fc4a23c86a5e8c666c7fd5a9))
* **client:** test du déplacement de carte, l'entrée ne passe pas derrière un bloc ([002155c](https://github.com/remi-deher/flux-de-fichiers/commit/002155c3d9e8f3d3e36a440807d9d78ef134b0ca))
* **frontend:** boutons de copie fonctionnels en HTTP et retour visuel ([e484ef2](https://github.com/remi-deher/flux-de-fichiers/commit/e484ef29f4a81b421f6c223c3db4cc0fa9363704))
* **frontend:** boutons de copie fonctionnels en HTTP et retour visuel « Copié » ([b5e118b](https://github.com/remi-deher/flux-de-fichiers/commit/b5e118b2551dd61a371f3de92244ed28faba383b))
* **frontend:** enregistrer la route d'état système et relever la limite de récursion des vues ([184add2](https://github.com/remi-deher/flux-de-fichiers/commit/184add25ab46de92264a67e07f8e2e4245e60b24))
* **frontend:** l'éditeur de flux charge et enregistre le graphe complet sans perte ([9007ce2](https://github.com/remi-deher/flux-de-fichiers/commit/9007ce29e93a4742e0bda77e68030f0122d3e37c))
* **secrets:** création de la clé de chiffrement sans fenêtre de lecture partielle ([ae11e4e](https://github.com/remi-deher/flux-de-fichiers/commit/ae11e4e2d66668af129feeafb080c91f5cc1f123))
* **tests:** retirer expect() des tests des blocs plugins (clippy expect_used) ([cc52dd6](https://github.com/remi-deher/flux-de-fichiers/commit/cc52dd6bd7c42be48e666605e7c8becc004007e4))
* **tool_run:** réessayer le lancement d'un exécutable encore occupé (ETXTBSY) ([56ff3a0](https://github.com/remi-deher/flux-de-fichiers/commit/56ff3a07ef6f1f39acd00c4206c9017cdc247d76))
* **tool_run:** réessayer le lancement quand l'exécutable est occupé ([fa2728c](https://github.com/remi-deher/flux-de-fichiers/commit/fa2728cbc82d8925cca114d71c15b69ee3018b48))

## [0.15.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.14.0...v0.15.0) (2026-11-12)


### Features

* **frontend:** kit de widgets, vue d'ensemble refondue et registres alignés ([c2f6430](https://github.com/remi-deher/flux-de-fichiers/commit/c2f643086317551e0b22be64d9b0535fd7550ab2))
* **frontend:** kit de widgets, vue d'ensemble refondue et registres alignés ([f03b56e](https://github.com/remi-deher/flux-de-fichiers/commit/f03b56e11191d49925e6e23f2068bde0c7a733ed))
* **plugins:** blocs Sonarr et Radarr, webhooks chiffrés, statut d'installation et README ([6156f7a](https://github.com/remi-deher/flux-de-fichiers/commit/6156f7ac8f2bc9ddcbb354c6679f3dd879620359))
* **plugins:** dépôts synchronisés, installateur d'outils et secrets chiffrés ([4742013](https://github.com/remi-deher/flux-de-fichiers/commit/47420136b5322b12790dffeef2bd3aa5734d4de3))
* **plugins:** dépôts synchronisés, installateur, secrets chiffrés et blocs *arr ([6665d39](https://github.com/remi-deher/flux-de-fichiers/commit/6665d39be013c8527885a24c3b20abf6e36804e5))
* **plugins:** installer le remux avant un job et retirer les outils de l'image ([de19dd9](https://github.com/remi-deher/flux-de-fichiers/commit/de19dd9e7051d4121df6b62d428be7e14362893d))
* **plugins:** installer le remux avant un job et retirer mkvtoolnix et mediainfo de l'image ([b0c6a3d](https://github.com/remi-deher/flux-de-fichiers/commit/b0c6a3dbf689a3eb30c1330224237f3bee187188))

## [0.14.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.13.0...v0.14.0) (2026-11-12)


### Features

* **frontend:** fusionner les volets du tiroir des bibliothèques et masquer la sous-navigation d'un volet unique ([faa9202](https://github.com/remi-deher/flux-de-fichiers/commit/faa9202be9703f454ba3aae9e4247d3969d12421))
* **frontend:** refonte de la vue d'ensemble ([2af0e69](https://github.com/remi-deher/flux-de-fichiers/commit/2af0e696da610f96febfabd1ff048fc57eb212c5))
* **frontend:** refonte de la vue d'ensemble et token --danger-text ([14f5a9d](https://github.com/remi-deher/flux-de-fichiers/commit/14f5a9d40f98713aa132a5e0c32aad08a61c5b29))
* **frontend:** tiroir des bibliothèques sans sous-navigation ([05d0bc7](https://github.com/remi-deher/flux-de-fichiers/commit/05d0bc78119e8885444bee205dc48492bc1618d1))


### Bug Fixes

* **frontend:** définir le token --danger-text utilisé par les boutons et textes d'erreur ([4ae51ee](https://github.com/remi-deher/flux-de-fichiers/commit/4ae51ee2523a94d1ef03559c15e25cd65c8d9022))

## [0.13.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.12.0...v0.13.0) (2026-11-12)


### Features

* **backend:** aligner les jobs sur le schéma et séparer les budgets HTTP ([f313853](https://github.com/remi-deher/flux-de-fichiers/commit/f313853a92e66d4ca735e51c316f3d778353cf14))
* bibliothèques déclarées et poste de pilotage à jour ([92e7237](https://github.com/remi-deher/flux-de-fichiers/commit/92e7237289b03171abfbeaf6084872ad87200cb8))
* déclarer les bibliothèques avec un flux et un chemin par nœud ([8002081](https://github.com/remi-deher/flux-de-fichiers/commit/80020817edca82ea62d9b646023b44a488617e20))
* drawer d'inspection, sonde catalogue et identifiants app/pack ([3161528](https://github.com/remi-deher/flux-de-fichiers/commit/3161528881c714fec4829912916e4df9c9e561ef))
* **frontend:** appliquer les standards modernes Google et Apple (PWA, Safe Areas, Touch Targets, Micro-Kit UI) ([691390a](https://github.com/remi-deher/flux-de-fichiers/commit/691390a48fa9ae5a1535218b56d385e0fb51d8c2))
* **frontend:** barre de recherche contextuelle dynamique réactive à la page active ([f80a029](https://github.com/remi-deher/flux-de-fichiers/commit/f80a029cfb559d110fe983128f4aac199894de13))
* **frontend:** capsule AppSubnav (Watchdeck) et passage intégral en SSE ([c6882ae](https://github.com/remi-deher/flux-de-fichiers/commit/c6882ae6126ebf8c7d818927cc6e9594de10388b))
* **frontend:** centrage parfait de la capsule AppSubnav et suppression de tous les boutons de synchronisation manuelle ([c38c376](https://github.com/remi-deher/flux-de-fichiers/commit/c38c376c655717b92a067b5c847da1656923325e))
* **frontend:** centrer la barre de recherche via grille CSS et transformer le bouton déplier en icône ([c2404ca](https://github.com/remi-deher/flux-de-fichiers/commit/c2404cad5d3230814dd35ed2cdd4284e6c0c5c66))
* **frontend:** header modernisé avec glassmorphism, recherche Ctrl+K et icônes Lucide ([9ec98bd](https://github.com/remi-deher/flux-de-fichiers/commit/9ec98bdcb0fc771530c93ae508735ffa46fd64b8))
* **frontend:** header modernisé avec glassmorphism, recherche Ctrl+K et icônes Lucide ([9d38434](https://github.com/remi-deher/flux-de-fichiers/commit/9d384349b42094483e327f016810d24dd42b8376))
* **frontend:** intégration de la capsule AppSubnav (Watchdeck) et suppression des boutons actualiser au profit du SSE ([90ba68e](https://github.com/remi-deher/flux-de-fichiers/commit/90ba68e15937f49d548cae140385a1c305047282))
* **frontend:** position fixe absolue de la recherche, actions ancrées à droite et statut des workers en ligne ([8a65333](https://github.com/remi-deher/flux-de-fichiers/commit/8a653338a9614fc2838bfe5fb5e8552121018acb))
* **frontend:** refonte de la page d'accueil en Hub 'Vue d'ensemble' avec statistiques réactives ([9f51a56](https://github.com/remi-deher/flux-de-fichiers/commit/9f51a569867f137eecb2be56eb97092f686c742d))
* **frontend:** refonte du hub d'accueil en Vue d'ensemble avec statistiques réactives ([1c14538](https://github.com/remi-deher/flux-de-fichiers/commit/1c1453872beb786748db8ef045064ed582aad9b2))
* **frontend:** standards Google/Apple et Micro-Kit UI ([58fcb2b](https://github.com/remi-deher/flux-de-fichiers/commit/58fcb2bbd764dc1a07fa683980989fc769e85eb4))
* **frontend:** suppression du bouton actualiser et réactivité temps réel SSE universelle ([86e6235](https://github.com/remi-deher/flux-de-fichiers/commit/86e623552343899c933e79d7b34a96fe81e31ba6))
* **frontend:** supprimer le bouton actualiser du header et généraliser le temps réel SSE ([e520dd1](https://github.com/remi-deher/flux-de-fichiers/commit/e520dd1ffe443b6c80d689b63dce2f6ff838b4d5))
* **frontend:** TopCommandBar flottante et auto-hide au scroll (Watchdeck) ([a9f0a81](https://github.com/remi-deher/flux-de-fichiers/commit/a9f0a811c2450b7851581035861f19ddeb8ec415))
* **frontend:** TopCommandBar flottante unifiée avec auto-hide au scroll et compteur de recherche inspirés de Watchdeck ([1888fed](https://github.com/remi-deher/flux-de-fichiers/commit/1888fed692d394c6f0e613cc52b9bb696b8a2454))
* **frontend:** un registre commun pour bibliothèques, flux, nœuds et catalogue ([deb8193](https://github.com/remi-deher/flux-de-fichiers/commit/deb8193c72f4cb5239b553f265f5683fa42e4b78))
* **frontend:** verrouiller la position fixe du header, centrer la recherche et afficher les workers actifs ([1580cbb](https://github.com/remi-deher/flux-de-fichiers/commit/1580cbb2df5a2113ecf5a5f59af664495bfb9f1c))


### Bug Fixes

* **frontend:** integrer les design tokens :root et resets W3C ([7fd3aa2](https://github.com/remi-deher/flux-de-fichiers/commit/7fd3aa2f1cd06abcc490b065f08f74b146b0d656))
* **frontend:** integrer les design tokens :root et resets W3C dans les styles consolides ([be02a8b](https://github.com/remi-deher/flux-de-fichiers/commit/be02a8bf062f59f35f153a4355524b660aea85a9))

## [0.12.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.11.0...v0.12.0) (2026-11-12)


### Features

* **client:** migrer en Rust pur (zéro JS, zéro CSS) et binaire dev ([bf733bf](https://github.com/remi-deher/flux-de-fichiers/commit/bf733bf96a06c381c339041d5b683fc46e82332c))
* **client:** migrer la logique client en Rust pur et supprimer static/js ([951c220](https://github.com/remi-deher/flux-de-fichiers/commit/951c2208e2136f83e89445c0d6e504d454b8634b))
* **client:** migrer la logique client en Rust pur et supprimer static/js ([7f567d4](https://github.com/remi-deher/flux-de-fichiers/commit/7f567d40d224b50c139ec90e67e85967f3513298))
* **dev:** binaire Rust natif pour l'environnement de développement et configuration Linguist 100% Rust ([4cc8b54](https://github.com/remi-deher/flux-de-fichiers/commit/4cc8b54fccde05b5b6574918882666bf0584fdf6))
* **test:** éliminer les scripts orphelins et ajouter tests/api_workflows.rs ([d99bbca](https://github.com/remi-deher/flux-de-fichiers/commit/d99bbcad826b549ff5c1ea4709fcffb5f84c871b))
* **test:** éliminer les scripts orphelins et ajouter tests/api_workflows.rs ([d01607b](https://github.com/remi-deher/flux-de-fichiers/commit/d01607b1aaa31b738957f7f58ccfada5fe85f626))
* **test:** éliminer les scripts orphelins et ajouter tests/api_workflows.rs ([7d429c2](https://github.com/remi-deher/flux-de-fichiers/commit/7d429c2bf7f087e6cd363b2912124548076d3725))


### Bug Fixes

* **clippy:** implementer std::str::FromStr pour JobErrorCode ([9b83a1d](https://github.com/remi-deher/flux-de-fichiers/commit/9b83a1dde59af20b8c8e98daf6ddd9bd3ba87e38))
* resolution securisee de INPUT_DIR pour conteneur non-root et logs smoke test ([73bec09](https://github.com/remi-deher/flux-de-fichiers/commit/73bec09a80bc34f0ec43692ab6c6f13cd7f44a68))
* **test:** assurer l'existence de input_dir dans test_app ([b86a0a8](https://github.com/remi-deher/flux-de-fichiers/commit/b86a0a8f42f216a339335d3403ea62384741c0f0))

## [0.11.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.10.0...v0.11.0) (2026-11-11)


### Features

* **theme:** migrer l'ensemble des styles vers du css-in-rust type et supprimer static/css ([4e81cb4](https://github.com/remi-deher/flux-de-fichiers/commit/4e81cb4cdea8386f0c66a5fa225f60a5df89420f))
* **theme:** migrer les styles en CSS-in-Rust pur et supprimer static/css ([f0fe7f4](https://github.com/remi-deher/flux-de-fichiers/commit/f0fe7f40cad4a441f2799c300d064c5fb7fb368b))
* **theme:** transition CSS-in-Rust pur et suppression des fichiers CSS ([019678d](https://github.com/remi-deher/flux-de-fichiers/commit/019678d01de8813223fd28cde8abb45c5db3f1cc))

## [0.10.0](https://github.com/remi-deher/flux-de-fichiers/compare/v0.9.1...v0.10.0) (2026-11-11)


### Features

* **subnav, multi-repos:** stabiliser la sous-navigation et la gestion multi-depots ([18a34eb](https://github.com/remi-deher/flux-de-fichiers/commit/18a34eb5578338f8ceeb29c23b6f6261a9ccc6ee))
* **ui:** integrer subnav tripartite, sous-navigation noeuds et workspace flows ([a491bb9](https://github.com/remi-deher/flux-de-fichiers/commit/a491bb9cbc32063362dd09df58f833356e39277c))


### Bug Fixes

* **activity:** deplacer job-detail-dialog au niveau racine du panel ([b78b795](https://github.com/remi-deher/flux-de-fichiers/commit/b78b7959b5b73d0eb387a434f5eae081b669864c))
* **activity:** positionner job-detail-dialog au niveau racine du panel ([4486ce7](https://github.com/remi-deher/flux-de-fichiers/commit/4486ce7dd6cccf1bca38fded9c249541d0d50a64))
* **ui:** corriger la syntaxe css responsive de subnav-actions ([12488f4](https://github.com/remi-deher/flux-de-fichiers/commit/12488f43cd07515e1326fcecae14a9cda8c53856))
* **ui:** corriger la syntaxe css responsive de subnav-actions et ajouter les regles mobile ([41ad675](https://github.com/remi-deher/flux-de-fichiers/commit/41ad67598f32a68bb18e3ed121b96dec03ed935b))

## [0.9.1](https://github.com/remi-deher/flux-de-fichiers/compare/v0.9.0...v0.9.1) (2026-11-11)


### Bug Fixes

* **ui:** afficher l'explorateur de medias par defaut dans la subnav bibliotheques ([84a087e](https://github.com/remi-deher/flux-de-fichiers/commit/84a087ea7f3d02f0f5c5b1740ffaeffe4a99692f))
* **ui:** afficher l'explorateur de medias par defaut dans la subnav bibliotheques ([51f7295](https://github.com/remi-deher/flux-de-fichiers/commit/51f72953660471514b72447a629d87d23b4a9fc8))

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
