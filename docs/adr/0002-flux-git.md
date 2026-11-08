# 0002 — Flux Git dev / test / main

Statut : accepté par l'utilisateur.

```
dev  ──●──●──●──   push direct, aucune CI : sauvegarde rapide du code
        \
test ────●──────   PR dev→test : fmt, clippy, tests, build → fusion auto
          \        → Docker Hub :test et :test-<sha>
main ──────●────   PR test→main → release-please → tag vX.Y.Z
                   → l'image test-<sha> est promue en :X.Y.Z, :X.Y, :latest
```

## Règles

- `dev` reçoit des push directs, sans CI. Ni suppression ni force-push.
- `test` n'accepte que les PR depuis `dev` (job `source-is-dev`). Si
  `fmt-clippy-tests`, `docker-build` et `docker-scenarios` sont verts, le workflow fusionne la PR
  lui-même (merge commit) et publie l'image `:test` et `:test-<sha12>`.
- `main` n'accepte que les PR depuis `test` ou la PR de release de
  release-please. La release est construite une seule fois puis promue : aucun
  recompilage, la release échoue si l'image `test-<sha>` est absente.
- Toutes les fusions se font en merge commit. Un squash ou un rebase ferait
  diverger les branches longues et créerait de faux conflits à chaque PR.
- Après une release, main est fusionnée dans dev (version et CHANGELOG).
- Correctif urgent : même chemin `dev → test → main`, pas de raccourci.
- Les branches de travail (`feat/…`, `fix/…`) sont supprimées automatiquement
  après fusion dans `dev` ; `dev`, `test` et `main` sont protégées contre la
  suppression.

## Optimisations de la CI

- L'image est construite une seule fois, pendant la PR, et poussée sous
  `pr-<sha>`. Après la fusion elle est renommée `:test` et `:test-<sha du merge>`
  sans recompiler. Les tags `pr-*` non fusionnés restent sur Docker Hub et
  peuvent être purgés.
- Si la PR ne modifie aucun chemin du build (`src/`, `migrations/`, `tests/`,
  `Cargo.*`, `Dockerfile`, `.dockerignore`, `rust-toolchain.toml`, `test.yml`,
  `scripts/`, `deploy/`, `compose.yaml`, `package.json`), les vérifications
  requises restent vertes mais sautent leurs étapes lourdes, et `:test` est
  simplement renommée `:test-<sha du merge>`.
- Le Dockerfile accepte `CARGO_JOBS` (1 par défaut, 4 en CI).

## Versionning

release-please lit les commits conventionnels (`feat:`, `fix:`) et ouvre une
PR de release sur `main` (version de `Cargo.toml`, `Cargo.lock`,
`CHANGELOG.md`). Avant 1.0, un changement cassant incrémente la mineure.

## Limites connues

- Le binaire promu affiche la version de `Cargo.toml` au moment du build sur
  `test` (`--version`). Le tag de l'image fait foi.
- Les PR et pushes faits par `GITHUB_TOKEN` ne déclenchent pas d'autre
  workflow, sauf notamment `pull_request` pour `opened`, `synchronize` et
  `reopened` : ces événements créent des runs qui attendent une approbation.
  Les pushes restent sans déclenchement : fusion et publication se font donc
  dans le workflow `test`. La PR de release n'a pas de vérification obligatoire
  sur `main`.
- Image amd64 uniquement.

## Approbation de main-guard sur les PR de release

release-please utilise `GITHUB_TOKEN`. Avant de fusionner sa PR sur `main`,
un utilisateur avec le droit d'écriture doit sélectionner **Approve workflows
to run** dans la bannière de la PR, puis attendre que `main-guard` /
`source-is-test` soit vert sur le SHA courant. Refaire cette vérification si
release-please met à jour la PR. Le garde reste informatif, non requis.

Le run [42506874126](https://github.com/remi-deher/flux-de-fichiers/actions/runs/42506874126)
de la PR #4 n'avait ni job ni log : son annotation indiquait
« This workflow run required approval but was not approved before it expired. ».
Le message de `gh run view` suggérant un problème de fichier workflow était
générique ; le YAML et le script du garde n'étaient pas en cause. Consulter les
**Annotations** de la page du run lorsqu'aucun job n'a démarré.

Ce comportement est décrit dans la
[documentation GitHub](https://docs.github.com/en/actions/how-tos/writing-workflows/choosing-when-your-workflow-runs/triggering-a-workflow#triggering-a-workflow-from-a-workflow).
Il ne nécessite ni nouveau secret ni modification du déclencheur du garde.

## Prérequis

Secrets du dépôt GitHub : `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` (jeton
Docker Hub en lecture/écriture). Le dépôt Docker Hub `flux-de-fichiers` est
créé au premier push.
