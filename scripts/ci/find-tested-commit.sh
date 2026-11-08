#!/usr/bin/env bash
# Affiche le commit de la branche test livré dans la release.
# Usage : find-tested-commit.sh <commit-de-release> <branche-test>
# Parcourt les merges de main (premier parent) et renvoie le second parent du
# plus récent qui appartient à la branche test : c'est le commit dont l'image
# test-<sha> a été publiée par le workflow test.
set -euo pipefail

release_ref="${1:?commit de release manquant}"
test_ref="${2:?branche test manquante}"

git rev-list --first-parent --merges --parents "$release_ref" | while read -r _ _ second _; do
  if git merge-base --is-ancestor "$second" "$test_ref"; then
    echo "$second"
    exit 0
  fi
done | head -n1 | grep . || { echo "Aucun merge depuis $test_ref trouvé." >&2; exit 1; }
