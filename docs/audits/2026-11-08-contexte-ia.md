# Audit du contexte des sessions IA — 8 novembre 2026

Impact attendu : **moyen**. Le dépôt était déjà petit et ses consignes courtes.
Le gain porte surtout sur la recherche ciblée et les erreurs évitées, pas sur
une forte réduction du contexte permanent.

## Périmètre et mesure

Inspection : fichiers d'instructions, README, deux ADR, contrat API, structure,
manifestes, scripts, CI, tests et points d'entrée du code ; trois notes de mémoire,
configuration locale des hooks/MCP, catalogues des compétences, prompts et commandes.
Lecture complète des deux compétences Rust contradictoires et de `release-et-tags`.
Les autres compétences et prompts ont été examinés via leurs descriptions/aperçus,
sans charger leurs corps hors sujet. Aucun secret lu, aucun service lancé.

- État initial propre : 45 fichiers suivis ; un crate, pas de frontend implémenté.
- `AGENTS.md` : 32 lignes, 1 418 caractères ; `CLAUDE.md` : un import de 11 caractères.
- README : 6 587 caractères ; ADR : 3 741 ; OpenAPI : 11 936.
- `Cargo.lock` : 70 693 caractères, 2 884 lignes ; verrou généré le plus volumineux.
- `target/` : environ 2 Gio et 4 659 fichiers, déjà ignorés par Git.
- Mémoire : trois notes avec index ; huit compétences globales, deux prompts
  globaux ; deux commandes enregistrées dont `npm run dev`, aucune en cours.
- Aucun agent, commande ou compétence propre au dépôt versionné. Les hooks locaux
  observés appartiennent à AgentsRoom ; ils ne constituent pas de l'outillage projet.

Estimations : caractères Unicode divisés par quatre, arrondis. Ce ne sont ni une
tokenisation spécifique à un modèle ni une trace de consommation réelle. Les
fichiers disponibles ne sont pas tous injectés automatiquement. Les gains ci-dessous
sont conditionnels et se recouvrent : ne pas les additionner.

## Constats classés par impact

| Priorité / impact | Constat et preuve | Gain plausible | Traitement |
|---|---|---|---|
| 1 / élevé pour la fiabilité | Les deux compétences Rust affirment encore C#/.NET, SQLite et Angular ; `release-et-tags` propose un hotfix depuis un tag vers main et une release manuelle, contrairement à l'ADR 0002 et à `release.yml`. | Évite une investigation ou un mauvais changement de stack/flux ; nombre de tours non mesuré. | Rappel des décisions locales et renvoi à l'ADR dans les consignes. Bibliothèque globale laissée intacte. |
| 2 / moyen, fréquent | Pas de parcours par tâche : une exploration peut charger consignes, README, ADR et OpenAPI intégralement (~5 923 tokens). | ~4 600 tokens, soit 78 %, si remplacés par consignes + guide complet (~1 300), avant les lectures propres à la tâche. | Guide ciblé ajouté ; README et consignes y renvoient. |
| 3 / moyen, occasionnel | Lire tout `Cargo.lock` pour identifier la stack charge ~17 700 tokens ; parcourir `target/` expose des milliers de fichiers. | Jusqu'à ~17 700 tokens évités pour une lecture intégrale du verrou ; bruit du build évité sans chiffrage en tokens. | `Cargo.toml`, `rg --files` et recherches ciblées recommandés. Exclusions existantes conservées, aucun gain supplémentaire revendiqué sur `target/`. |
| 4 / moyen pour le temps | Rust, tests API et scénarios Docker ont des coûts et prérequis différents ; les scénarios suspendent des agents et redémarrent PostgreSQL. La CI ne les lance pas. | Évite un démarrage/build Docker inutile pour une modification documentaire ; durée variable, non remesurée. | Matrice de vérification selon la tâche ; limites de validation explicites. |
| 5 / faible | Cinq exclusions de fichiers `.agentsroom/*` sont couvertes par `.agentsroom/` ; règle Prisma héritée ; seule `.env.local` est couverte en plus de `.env`. | Quelques lignes en moins, prévention du bruit futur ; aucun gain significatif immédiat en tokens. | Redondances et règle Prisma retirées ; `.env.*` et configurations locales d'agents couvertes. |
| 6 / faible dans le dépôt, potentiel externe | Décisions répétées dans mémoire/README/ADR ; consignes de session et étiquette AgentsRoom très présentes. | Lire une seule note pertinente économise le reste de la mémoire ; contexte injecté externe non quantifié. | Lecture descendante de l'index rappelée. Aucun fichier de fonctionnement AgentsRoom modifié. |

## Modifications et choix conservés

- `AGENTS.md` réorganisé : 29 lignes, 1 437 caractères (~359 tokens contre ~354).
  Coût permanent presque stable (+5 tokens estimés), avec orientation par tâche.
- `docs/developpement.md` ajouté : 3 770 caractères (~942 tokens) **à la demande**,
  avec points d'entrée, validation proportionnée et pièges média/TLS.
- README : lien de contribution ajouté (+127 caractères). Pas de découpage de ses
  125 lignes initiales : il est cohérent et reste utile aux utilisateurs.
- `.gitignore` nettoyé et étendu aux variantes `.env.*` et aux configurations
  locales `.agents/`, `.grok/`, `.vibe/`, `.kimi-code/`.
- Conservés : sécurité, stack validée, contrat localhost:3000, HA différée,
  annonce en deux lignes, petites responsabilités, noms explicites, ADR acceptés,
  contrat OpenAPI complet, verrou Cargo, exclusions build/médias, import Claude.
- Aucun nouveau hook, script, compétence ou agent : le guide réutilise les commandes
  existantes ; ajouter une couche d'automatisation ici augmenterait la maintenance.
- Code source, scripts, manifests, CI, secrets et historique Git inchangés. Aucun commit.

## Avant / après pour une session

| Lecture | Avant | Après |
|---|---|---|
| Consignes permanentes du dépôt | ~357 tokens, import Claude inclus | ~362 tokens, import inclus |
| Orientation documentaire large | ~5 923 tokens si tout le corpus initial est ouvert | ~1 304 pour consignes + guide entier ; moins si seule une rubrique est lue |
| API, utilisation, décisions | Faciles à charger ensemble faute d'orientation | Rubrique du README, route OpenAPI ou ADR seulement si nécessaire |
| Mémoire AgentsRoom | Index et notes disponibles | Index puis note pertinente ; mécanisme existant conservé |
| Verrou et build | Déjà exclus des recherches usuelles pour le build | Même protection ; parcours ciblé désormais documenté |

Le volume documentaire total augmente volontairement avec le guide et cet audit.
Ces documents ne sont pas importés automatiquement : le gain dépend du respect
du parcours de lecture, pas d'une suppression de connaissance utile.

## Vérifications et possibilités restantes

Vérification documentaire : chemins et liens locaux, couverture des exclusions,
diff limité aux fichiers autorisés et `git diff --check`. Aucun test Rust ni
scénario Docker nécessaire pour ces modifications.

À traiter séparément : rendre les compétences globales Rust indépendantes d'un
ancien projet et adapter les compétences Git/release au flux local ; elles servent
aussi d'autres dépôts et n'ont pas été réécrites ici. La mémoire conserve également
une rubrique historique « propositions restant à finaliser » sur Axum/SQLx : les
décisions actuelles sont dans l'ADR 0001. Ne pas prendre cette rubrique pour l'état
du code. Les affirmations de tests passés en mémoire sont historiques.

La version OpenAPI 0.1.0 diffère de la version du paquet 0.2.0 ; aucun changement
automatique, car une version de contrat peut être indépendante de celle du binaire.
Si de longues sessions persistent, mesurer ensuite une trace réelle de chargement
avant de modifier les instructions globales ou l'injection d'outils AgentsRoom.
