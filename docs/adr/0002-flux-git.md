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
  `fmt-clippy-tests` et `docker-build` sont verts, le workflow fusionne la PR
  lui-même (merge commit) et publie l'image `:test` et `:test-<sha12>`.
- `main` n'accepte que les PR depuis `test` ou la PR de release de
  release-please. La release est construite une seule fois puis promue : aucun
  recompilage, la release échoue si l'image `test-<sha>` est absente.
- Toutes les fusions se font en merge commit. Un squash ou un rebase ferait
  diverger les branches longues et créerait de faux conflits à chaque PR.
- Après une release, main est fusionnée dans dev (version et CHANGELOG).
- Correctif urgent : même chemin `dev → test → main`, pas de raccourci.

## Versionning

release-please lit les commits conventionnels (`feat:`, `fix:`) et ouvre une
PR de release sur `main` (version de `Cargo.toml`, `Cargo.lock`,
`CHANGELOG.md`). Avant 1.0, un changement cassant incrémente la mineure.

## Limites connues

- Le binaire promu affiche la version de `Cargo.toml` au moment du build sur
  `test` (`--version`). Le tag de l'image fait foi.
- Les PR et pushes faits par `GITHUB_TOKEN` ne déclenchent pas d'autre
  workflow : fusion et publication se font donc dans le workflow `test`, et la
  PR de release n'a pas de vérification obligatoire sur `main`.
- Image amd64 uniquement.

## Prérequis

Secrets du dépôt GitHub : `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` (jeton
Docker Hub en lecture/écriture). Le dépôt Docker Hub `flux-de-fichiers` est
créé au premier push.
