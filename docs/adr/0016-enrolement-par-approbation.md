# 0016 — Enrôlement d'un worker par demande à approuver

Statut : proposé (14 nov. 2026), orientation validée par l'utilisateur ; à implémenter.

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
- Le contrat `flux-shared` expose le statut d'une demande (enum Serde).
