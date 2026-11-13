# Déploiement de l'installation de test (ADR 0011, phases 1 à 5)

Procédure pour `flux-essai` (centrale, `192.168.1.50`) et `flux-essai-worker` (worker `artemis-essai`,
`192.168.1.51`). Chaque étape modifiante demande une confirmation.

**Avancement au 13 nov. 2026** : les étapes A à F sont exécutées sur la centrale, avec l'accord de
l'utilisateur. `flux-essai` tourne sur `test-89e594c7451b`, installation vierge (`/health` 204,
`needsSetup: true`). Le worker est arrêté (étape B) et encore en `0.12.0`. Restent G (installation par
l'assistant, mot de passe saisi par l'utilisateur), H, I et J. Détails et retour arrière dans le journal
de Rashin du 13 nov. Écarts constatés à l'exécution :

- l'étape D passe par `midclt call -j app.update` avec `custom_compose_config` (le champ `image` seul
  change) ; la configuration d'origine est conservée dans `compose-flux-essai.avant-0011.json` ;
- la sauvegarde à chaud de `arr-config` (4,3 Mo) contenait le journal WAL de SQLite ; la sauvegarde à froid
  (`...-froid`, 178 Ko) est la bonne : base arrêtée proprement.

## État constaté

| Élément | Centrale (`.50`, `truenas`, TrueNAS 25.10.6) | Worker (`.51`, `ARTEMIS`, TrueNAS 25.10.7) |
| --- | --- | --- |
| Application | `flux-essai`, RUNNING | `flux-essai-worker`, RUNNING |
| Image actuelle | `mrcryllix/flux-de-fichiers:latest` (0.15.0, 12 nov.) | `mrcryllix/flux-de-fichiers:0.12.0` |
| Accès | HTTP `:18480` → conteneur `:3000` | aucun port publié |
| Données | `/mnt/DATA/flux-de-fichiers-test/essai25/` (`arr-config` = `/config`, `data`, `logs`, `input`, `frontend`) | `/mnt/SSD/flux-de-fichiers-test/essai25/` (`worker-work`, `worker-logs`, `worker-secrets/worker_key`) |
| Variables | `BIND`, `CONFIG_DIR`, `DATA_DIR`, `INPUT_DIR`, `LOCAL_WORKER`, `LOG_DIR`, `STATIC_DIR` | `COORDINATOR_URL=http://192.168.1.50:18480`, `WORKER_NAME=artemis-essai`, `WORKER_KEY_FILE=/run/secrets/worker_key`, `MAX_FILE_BYTES` |

Autres constats :

- L'interface est servie depuis le volume `frontend` (monté en lecture seule sur
  `/usr/share/flux-de-fichiers/frontend`) : il faut y recopier le `index.html` de la nouvelle image.
- Sur `.51`, un ancien `flux-essai` (centrale) est `STOPPED` : on n'y touche pas.
- Des sauvegardes `arr-config.bak-<version>` existent déjà : on suit la même convention.
- Le conteneur du worker est en lecture seule et n'a pas de volume `/config` : il fonctionne (le dossier
  existe dans l'image), mais la copie locale des comptes (connexion de secours) ne peut pas s'y écrire.
  Pour l'activer, ajouter un volume inscriptible sur `/config` à l'application du worker.
- Le worker démarre désormais un serveur HTTP sur `:3000` dans son conteneur. Pour ouvrir son interface
  depuis le réseau, publier un port (par exemple `18481:3000`) ; sans cela il reste joignable seulement
  par la centrale.

## Image à déployer

`mrcryllix/flux-de-fichiers:test-89e594c7451b` : pointe de `test`, CI verte (PR #92, #93 et #94), phases 2
à 5 incluses. L'étiquette `latest` est encore la 0.15.0 et **n'a pas** ces phases : la promotion
`test → main` n'est pas faite. Étiquette immuable recommandée pour l'essai.

## Procédure

Les commandes se lancent en SSH (`truenas_admin@<hôte>`). Forme `midclt` validée le 9 nov. : `-j`.

### A. Préparer (sans effet sur l'application)

1. Sur `.50`, tirer l'image : `sudo docker pull mrcryllix/flux-de-fichiers:test-89e594c7451b`.
2. Sur `.51`, la tirer aussi.
3. Sauvegarder la configuration de la centrale (copie, rien n'est supprimé) :
   `sudo cp -a /mnt/DATA/flux-de-fichiers-test/essai25/arr-config /mnt/DATA/flux-de-fichiers-test/essai25/arr-config.bak-0.15.0-avant-0011`.
4. Sauvegarder le secret du worker, sur `.51` :
   `sudo cp -a /mnt/SSD/flux-de-fichiers-test/essai25/worker-secrets /mnt/SSD/flux-de-fichiers-test/essai25/worker-secrets.bak-0.12.0`.

### B. Arrêter (interruption de service de l'essai)

1. Worker d'abord, sur `.51` : `midclt call -j app.stop '"flux-essai-worker"'`.
2. Centrale, sur `.50` : `midclt call -j app.stop '"flux-essai"'`.

### C. Remettre l'installation à zéro (destructif pour les comptes et clés de l'essai)

Le schéma de comptes a changé et il n'y a pas de migration des anciens secrets partagés (ADR 0011,
décision 4). Déplacer plutôt que supprimer, pour pouvoir revenir en arrière :

```bash
cd /mnt/DATA/flux-de-fichiers-test/essai25
sudo mv arr-config arr-config.avant-0011
sudo mkdir arr-config && sudo chown 10001:10001 arr-config && sudo chmod 700 arr-config
```

`data`, `input`, `logs` et `postgres` ne sont **pas** touchés.

### D. Mettre à jour l'image de la centrale

Interface TrueNAS : Applications → `flux-essai` → Modifier → image
`mrcryllix/flux-de-fichiers:test-89e594c7451b` → Enregistrer. Forme en ligne de commande, vérifiée le
13 nov. 2026 (TrueNAS 25.10.6) : lire `app.config`, enregistrer la configuration, ne changer que
`services.coordinator.image`, puis `midclt call -j app.update '"flux-essai"' '{"custom_compose_config": …}'`.
L'application reste arrêtée après la mise à jour.

### E. Recopier l'interface (sur `.50`)

```bash
c=$(sudo docker create mrcryllix/flux-de-fichiers:test-89e594c7451b)
sudo docker cp "$c":/usr/share/flux-de-fichiers/frontend/index.html /mnt/DATA/flux-de-fichiers-test/essai25/frontend/index.html
sudo docker rm "$c"
```

### F. Démarrer la centrale et vérifier

1. `midclt call -j app.start '"flux-essai"'`.
2. `curl -s -o /dev/null -w '%{http_code}\n' http://192.168.1.50:18480/health` → `204`.
3. `curl -s http://192.168.1.50:18480/api/v1/setup/status` → `needsSetup: true`, `nodeKind: central`.

### G. Installer la centrale

Ouvrir `http://192.168.1.50:18480` → « Installer une centrale » → nom, identifiant et mot de passe
administrateur. Le mot de passe circule en clair (HTTP) : réseau de confiance seulement.

### H. Clé du worker

1. Réglages → Clés d'API → créer une clé de rôle **Worker** nommée `artemis-essai` (même nom que
   `WORKER_NAME`). La clé n'est affichée qu'une fois.
2. Sur `.51`, l'écrire dans le secret sans la laisser dans l'historique du shell :

```bash
read -rs KEY && printf '%s' "$KEY" | sudo tee /mnt/SSD/flux-de-fichiers-test/essai25/worker-secrets/worker_key >/dev/null; unset KEY
```

### I. Mettre à jour et démarrer le worker (sur `.51`)

1. Applications → `flux-essai-worker` → Modifier → même étiquette d'image.
2. Facultatif : volume inscriptible sur `/config`, et port publié pour l'interface du worker.
3. `midclt call -j app.start '"flux-essai-worker"'`.

### J. Vérifier

- Réglages → Nœuds : `artemis-essai` apparaît avec une activité récente.
- Se connecter à l'interface du worker (si un port est publié) avec le compte administrateur : le tableau de
  bord affiche les tâches du nœud.
- Créer un compte « lecteur » et vérifier qu'il ne voit pas l'administration.
- Lancer une tâche de remux et vérifier qu'elle passe par `artemis-essai`.

## Retour arrière

1. Arrêter les deux applications (étape B).
2. Centrale : `sudo rm -r arr-config && sudo mv arr-config.avant-0011 arr-config`, remettre l'image
   `mrcryllix/flux-de-fichiers:latest` (ou la précédente) et recopier son `index.html` (étape E avec
   l'ancienne image).
3. Worker : restaurer `worker-secrets.bak-0.12.0`, remettre l'image `0.12.0`.
4. Redémarrer (centrale d'abord).

## Points à confirmer avant de commencer

- Accord pour les étapes B (arrêt), C (déplacement de `arr-config`), D et I (changement d'image).
- Étiquette d'image : `test-89e594c7451b` (recommandé) ou attendre la promotion `test → main`.
- Volume `/config` et port publié pour le worker : oui ou non.
