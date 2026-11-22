# 0016 — Enrôlement d'un worker par demande à approuver

Statut : accepté et implémenté (22 nov. 2026), sauf les écarts listés dans « Vérification point par
point » ; proposé le 14 nov. 2026, orientation validée par l'utilisateur.

## Contexte

L'ADR 0011 (décision 5) enrôle un worker avec l'adresse du central, l'identifiant et le mot de
passe d'un administrateur saisis **sur la page d'installation du worker**. Deux défauts :

- le mot de passe administrateur voyage du worker vers le central ;
- rien ne s'automatise sans mettre ce mot de passe dans le `compose`, et lors de l'essai la clé
  du worker a dû être copiée à la main dans un fichier secret.

Exigences de l'utilisateur (14 nov. 2026) : le compte ne sert qu'**une fois**, à confirmer que
l'on a le droit d'accéder au central ; **aucune valeur de `compose` à renseigner, aucune clé à
chercher dans les journaux** ; préférence pour une **demande à approuver**.

## Décision

1. **Le worker demande, un administrateur approuve depuis le central.** Le compte confirme les
   droits (l'administrateur est déjà connecté au central) ; le mot de passe ne touche plus jamais
   un worker.
2. **Parcours** :
   1. un conteneur non configuré démarre en worker non installé (comportement actuel) et sert sa
      page de premier lancement ; l'utilisateur y saisit **l'adresse du central**, une fois, dans
      l'interface (jamais dans le `compose`), et un nom facultatif (hôte par défaut) ;
   2. le worker envoie `POST /api/v1/workers/requests` (nom, version, secret aléatoire dont seul
      le haché est transmis) et affiche « en attente d'approbation » ;
   3. l'écran Workers du central affiche la demande « en attente » avec le nom, l'adresse source
      et la version du worker ; l'administrateur **approuve ou refuse** (pas de code à comparer :
      la fenêtre de 15 minutes ouverte par lui suffit à ne viser qu'une demande) ;
   4. le worker interroge `GET /api/v1/workers/requests/{id}` avec son secret ; à l'approbation, le
      central crée la clé du nœud et la lui remet **une seule fois** ; le worker la range dans
      `/config` (`worker.key`, 0600) et passe en fonctionnement normal.
3. **Fenêtre d'acceptation** : le central n'accepte aucune demande par défaut. Le bouton « Ajouter
   un worker » ouvre une fenêtre de 15 minutes ; hors fenêtre, la route répond refus sans créer
   d'état. Cela évite qu'un client du réseau remplisse la liste d'attente.
4. **Garde-fous** : demandes limitées en nombre et en débit par adresse ; expiration d'une demande
   à 15 minutes ; refus ou expiration effacent la demande ; une demande approuvée ne peut être
   retirée qu'une fois ; l'écran d'approbation montre le nom, l'adresse source et la version pour
   ne pas approuver la mauvaise demande.
5. **Clés de nœud invisibles.** La clé d'un worker est créée, stockée et révoquée par l'écran
   Workers ; elle n'apparaît plus dans « Clés d'API », réservé aux clés personnelles (ADR 0015).
6. **Secours** : l'enrôlement par identifiant et mot de passe saisis sur le worker (ADR 0011,
   décision 5) reste disponible, pour un central injoignable depuis le navigateur de l'administrateur.
7. **Découverte** : l'annonce du central par mDNS (`_flux-central._tcp`) pour préremplir l'adresse
   est un bonus facultatif ; elle ne fonctionne pas dans un réseau Docker en pont (cas de TrueNAS),
   donc la saisie de l'adresse reste le chemin de référence.

## Limites

- En HTTP simple, la clé remise au worker circule en clair sur le réseau local (comme le mot de
  passe aujourd'hui) : un TLS devant le central reste conseillé.
- Sans code de vérification, la protection contre l'approbation de la mauvaise demande repose sur
  la fenêtre de 15 minutes et sur le nom, l'adresse et la version affichés ; un attaquant du réseau
  local qui demande pendant la fenêtre apparaît dans la liste, à refuser.
- L'adresse du central est encore saisie une fois par worker ; la supprimer demande la découverte
  automatique (point 7).

## Conséquences

- Nouvelle table `worker_enrollment_requests` (haché du secret, nom, adresse source,
  expiration, statut) et routes côté central et worker, avec tests natifs (expiration, fenêtre
  fermée, limite de débit, remise unique de la clé).
- L'écran Workers gagne « Demandes en attente » et le bouton « Ajouter un worker » (le bloc
  `compose` du lot A du réseau devient une simple explication sans valeur à copier).
- Le contrat `forgearr-shared` expose le statut d'une demande (enum Serde).

## Vérification point par point (22 nov. 2026)

Vérification faite dans le code et les tests natifs ; le parcours complet en conteneurs est contrôlé
par `main-checks.yml`.

| Point | État |
| --- | --- |
| 1. Demande du worker, approbation au central | Fait |
| 2. Parcours | Fait. La page d'une installation vierge propose le choix central ou worker ; avec la commande `worker`, seulement l'enrôlement |
| 3. Fenêtre de 15 minutes fermée par défaut | Fait (403 hors fenêtre, sans état créé) |
| 4. Garde-fous | Fait : 20 demandes en attente, 5 par minute et par adresse, expiration, retrait unique. Une demande refusée n'est effacée qu'à la lecture suivante du worker ou à l'expiration |
| 5. Clés de nœud invisibles | **Partiel** : la clé n'est ni affichée ni créée à la main, mais elle reste listée et révocable dans « Clés d'API » ; retirer un worker de l'écran Workers ne révoque pas sa clé |
| 6. Secours par identifiants | Fait |
| 7. Découverte mDNS | **Non fait** (facultatif) |

## Suppression du mode manuel (22 nov. 2026)

Décision de l'utilisateur : l'enrôlement devient le seul moyen d'ajouter un worker.

- `WorkerConfig::load` ne lit l'identité que dans `/config`. `WORKER_KEY`, `WORKER_KEY_FILE`,
  `WORKER_NAME` et `CENTRAL_URL` ne sont plus lues. La clé **par worker**, créée à l'enrôlement et rangée
  dans `/config/worker.key`, demeure.
- La commande `worker` sans enrôlement sert la page d'enrôlement, sans worker intégré ni installation
  possible en central. Si `WORKER_KEY` ou `WORKER_KEY_FILE` est encore renseignée, un avertissement qui
  nomme la variable (jamais sa valeur) est écrit dans les journaux et sur la sortie du conteneur.
  `/config` non inscriptible, ou contenant un central installé, arrête le démarrage avec une erreur explicite.
  Avant l'enrôlement, rien n'est créé dans `/data` ni `/input` : les dossiers de travail sont sous `/config/worker-setup`.
- L'image n'a plus de commande par défaut : `central` l'emportait sur le rôle conservé, et un worker
  enrôlé sans commande redevenait un central vierge au redémarrage du conteneur.
- L'API refuse de créer une clé de rôle worker ; celles qui existent restent visibles et révocables.
- Remettre une clé à un worker révoque, dans la même transaction, ses autres clés actives (même nom, rôle
  worker). Conséquence : approuver une demande qui porte le nom d'un worker en service le déconnecte ;
  l'écran Workers le signale sur la demande et demande confirmation.
- L'enrôlement fait confiance à l'autorité privée `CA_CERT`, comme le worker.

### Migrer un worker déployé à la main

1. Relever son nom exact dans l'écran Workers et attendre la fin de ses tâches, puis mettre à jour le
   **central**. Les clés en place continuent de fonctionner avec les workers non mis à jour.
2. Dans le déploiement du worker : retirer `WORKER_KEY`, `WORKER_KEY_FILE`, `WORKER_NAME`, `CENTRAL_URL` et le
   secret monté ; monter un volume **persistant, inscriptible par l'utilisateur 10001 et propre au worker**
   sur `/config` ; publier le port 3000. Garder les autres volumes et les chemins des médias.
3. Mettre à jour l'image et démarrer : le worker affiche sa page d'enrôlement.
4. Sur le central, écran Workers, « Ajouter un worker » : ouvrir la fenêtre (15 minutes).
5. Sur la page du worker, saisir l'adresse du central et **le même nom** (casse comprise), puis
   approuver la demande sur le central.
6. Vérifier que le worker est en ligne avec ses dossiers : l'enregistrement se fait par nom, donc son
   identifiant, ses réglages, ses bibliothèques et son historique sont conservés. Son ancienne clé est révoquée si
   elle portait ce nom ; sinon la révoquer dans « Clés d'API ».
7. Supprimer l'ancien fichier de clé de l'hôte.

Retour arrière avant l'étape 5 : remettre l'ancienne image et l'ancienne configuration. Après, l'ancienne
clé est révoquée et ne peut plus être recréée à la main : il faut enrôler.
Pour enrôler de nouveau un worker (clé révoquée, central remplacé) : l'arrêter, supprimer `worker.json` et
`worker.key` de son `/config`, redémarrer.
