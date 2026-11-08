# Purge des images de PR Docker Hub

Le workflow `dockerhub-pr-cleanup.yml` s'exécute quotidiennement à 04:23 UTC
depuis la branche par défaut (`dev`). Il supprime seulement les tags correspondant
exactement à `pr-[0-9a-f]{12}`, dont `last_updated` est antérieur à la rétention.
Les têtes des PR encore ouvertes vers `test` sont exclues. Une ancienne image de
PR fermée ou remplacée devient donc éligible après la rétention, au prochain
passage quotidien. Aucun événement de fermeture n'est requis : les fusions par
`GITHUB_TOKEN` ne déclenchent pas d'autre workflow.

La variable GitHub de dépôt `PR_IMAGE_RETENTION_DAYS` fixe cette durée en jours
entiers strictement positifs (14 par défaut). L'âge est revérifié avant suppression.
Le lancement manuel propose `dry_run=true` par défaut : il affiche les candidats
sans les supprimer. Examiner cette simulation avant le premier passage réel.

La suppression utilise l'API Docker Hub **par nom de tag**, jamais l'API Registry
de suppression d'un manifeste par digest. `test`, `test-<sha>`, `X.Y.Z`, `X.Y`,
`latest` et tous les autres noms hors liste autorisée sont préservés. Après chaque
suppression, les digests des tags protégés sont relus et comparés. Si un
`test-<sha12>` partage un digest supprimé, le workflow exécute `docker pull` puis
`docker buildx imagetools create --dry-run` pour vérifier téléchargement et
promotion sans publier de version. Un échec arrête le workflow ; une publication
concurrente qui modifie un tag protégé peut aussi faire échouer cette comparaison.

Les secrets GitHub existants `DOCKERHUB_USERNAME` et `DOCKERHUB_TOKEN` sont
réutilisés. Si Docker Hub refuse la suppression (401/403), conserver le message
d'erreur du run et demander à l'utilisateur un jeton disposant du droit de
suppression, à fournir uniquement dans le secret GitHub `DOCKERHUB_TOKEN`.
Ne jamais copier sa valeur dans le dépôt ou un commentaire.

Vérification locale, sans réseau ni secrets :

```sh
python3 -m unittest discover -s scripts/ci -p 'test_dockerhub_cleanup.py' -v
```

La simulation ne prouve pas le droit de suppression. La preuve réelle est le run
non simulé qui supprime un `pr-*` partageant le digest d'un `test-<sha12>` et réussit
les contrôles de téléchargement et de promotion. Ne pas déclarer cette preuve
acquise sur la seule base des tests locaux.
