# 0002 — Flux Git dev / test / main

Statut : accepté par l'utilisateur.

```
dev  ──●──●──●──   PR de branche de travail : fmt, clippy, tests (feature.yml)
        \          (fusion manuelle) ; chaque push sur dev ouvre dev→test (promote.yml)
test ────●──────   PR dev→test : fmt, clippy, tests, build + essai /health sans publier
          \        → fusion auto → image construite et publiée :test et :test-<sha>
          \        → ouverture auto de test→main
main ──────●────   PR test→main : main-checks (image test-<sha> présente, démarre)
                   → fusion auto → release-please ouvre sa PR, fusionnée seule
                   → tag vX.Y.Z, l'image test-<sha> est promue en :X.Y.Z, :X.Y, :latest
```

## Règles

- `dev` reçoit les branches de travail (`feat/…`, `fix/…`) par PR. Le workflow
  `feature.yml` y exécute `cargo fmt --check`, clippy (`-D warnings`, natif et WASM),
  les tests et la vérification que `frontend/dist/index.html` est à jour (décision
  utilisateur du 14 nov. 2026). Pas d'image Docker : elle reste construite sur la PR
  `dev → test`. Ni suppression ni force-push. Rendre `fmt-clippy-tests` obligatoire
  sur `dev` est un réglage GitHub, hors dépôt.
- `test` n'accepte que les PR depuis `dev` (job `source-is-dev`). Si
  `fmt-clippy-tests` (fmt, clippy, `cargo test`) et `docker-build` (l'image est
  construite et répond sur `/health`, sans être publiée) sont verts, le workflow
  fusionne la PR lui-même (merge commit), construit et publie l'image `:test` et
  `:test-<sha12>` du commit fusionné, puis ouvre la PR `test → main` et active sa
  fusion automatique.
- `main` n'accepte que les PR depuis `test` ou la PR de release de
  release-please. La release est construite une seule fois puis promue : aucun
  recompilage, la release échoue si l'image `test-<sha>` est absente.
  Sur la PR `test → main`, `main-checks` / `image-test-ready` le vérifie en
  avance : il tire `test-<sha12>`, contrôle que sa révision est bien livrée
  par `test`, puis démarre le coordinateur et le worker (`compose.yaml`) et
  attend `/health`. `:latest` n'est publié qu'au merge de la release. Ajouter
  `image-test-ready` aux vérifications requises de la protection de `main`
  (réglage GitHub, hors dépôt).
- Automatisation : le secret `RELEASE_TOKEN` (jeton d'un compte avec droit d'écriture,
  PAT ou GitHub App) sert à release-please, à la fusion de `dev → test` et à l'ouverture
  et la fusion automatique des PR `dev → test`, `test → main` et de release. Avec
  `GITHUB_TOKEN`, GitHub ne lance aucun workflow sur ces PR et attend une approbation
  manuelle. La fusion automatique attend les vérifications requises du ruleset.
  La fusion des PR de branche de travail vers `dev` reste manuelle (relecture).
- Toutes les fusions se font en merge commit. Un squash ou un rebase ferait
  diverger les branches longues et créerait de faux conflits à chaque PR.
- Après une release, main est fusionnée dans dev (version et CHANGELOG).
- Correctif urgent : même chemin `dev → test → main`, pas de raccourci.
- Les branches de travail (`feat/…`, `fix/…`) sont supprimées automatiquement
  après fusion dans `dev` ; `dev`, `test` et `main` sont protégées contre la
  suppression.

## Optimisations de la CI

- Aucune image n'est publiée pendant la PR : elle est construite pour l'essai
  `/health`, avec le cache GitHub Actions, puis reconstruite à la fusion à partir de
  ce cache et poussée sous `:test` et `:test-<sha du merge>`. Plus de tags `pr-*`
  (ceux déjà présents sont purgés par `dockerhub-pr-cleanup`).
- Si la PR ne modifie aucun chemin du build (`src/`, `frontend/`, `client/`, `shared/`,
  `migrations/`, `tests/`, `Cargo.*`, `Dockerfile`, `.dockerignore`,
  `rust-toolchain.toml`, `test.yml`, `.github/actions/`, `scripts/`, `deploy/`,
  `compose.yaml`, `package.json`), les vérifications requises restent vertes mais
  sautent leurs étapes lourdes, et `:test` est simplement étiquetée `:test-<sha du merge>`.
- Le Dockerfile accepte `CARGO_JOBS` (1 par défaut, 4 en CI).

## Versionning

release-please lit les commits conventionnels (`feat:`, `fix:`) et ouvre une
PR de release sur `main` (version de `Cargo.toml`, `Cargo.lock`,
`CHANGELOG.md`). Avant 1.0, un changement cassant incrémente la mineure.

## Limites connues

- Le binaire est compilé sur `test`, avant que release-please n'avance `Cargo.toml` : sa version
  compilée est celle de la release précédente. La promotion ajoute donc une couche minimale
  (`ENV FLUX_VERSION`, libellé `org.opencontainers.image.version`, aucune compilation) et
  l'application affiche `FLUX_VERSION` ; la version compilée ne sert que de repli et de
  témoin de cohérence avec l'interface WASM. Le condensat de `X.Y.Z` diffère de `test-<sha>`.
- Les PR et pushes faits par `GITHUB_TOKEN` ne déclenchent pas d'autre workflow :
  d'où `RELEASE_TOKEN`. Si le secret est absent ou expiré, les PR de la chaîne
  restent sans vérifications ; le renouveler dans les secrets du dépôt.
- release-please peut ignorer un commit `feat:` ou `fix:` daté d'avant la dernière release, même fusionné
  après elle (22 nov. 2026, #456 : `main` à jour mais aucune PR de release). Avant de fusionner dans `dev`
  une branche commencée avant une release, vérifier que la PR de release s'ouvre ; sinon, pousser un
  commit conventionnel récent qui porte le même message de version.
- Image amd64 uniquement.

## Prérequis

Secrets du dépôt GitHub : `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` (jeton
Docker Hub en lecture/écriture), `RELEASE_TOKEN` (jeton GitHub, droits
`contents` et `pull-requests` en écriture). « Allow auto-merge » doit être activé. Le dépôt Docker Hub `forgearr` est
créé au premier push.
