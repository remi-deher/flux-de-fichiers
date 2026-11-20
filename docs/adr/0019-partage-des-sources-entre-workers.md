# 0019 — Partage des sources entre workers : HTTP direct, repli par le central

Statut : implémenté (19 nov. 2026) ; le repli par le central (point 6) est remplacé par l'[ADR 0022](0022-artefacts-retenus-et-tires-en-direct.md)
(proposé, 20 nov. 2026). Transport validé par l'utilisateur (HTTP direct avec repli par le
central ; WebDAV écarté). Complète l'[ADR 0018](0018-relais-de-tache-par-bloc.md), qui relaie une
tâche vers un autre worker mais ne sait pas encore lui faire parvenir une source locale.

## Contexte

Aujourd'hui une source n'est lisible que de deux façons : par le central (`GET /api/v1/jobs/{id}/source`,
bail exigé) ou par un worker qui monte lui-même le dossier (`verified_job_source_mounts`). Un job dont
la source est locale à un worker n'est attribuable qu'à lui (filtre de `claim`). Un relais vers un
autre worker échoue donc dès que ce dernier n'a pas le même montage.

Les workers ne font que se connecter au central : aucun n'écoute, aucun ne sait servir un fichier.

Le besoin (utilisateur) : le worker qui a un accès direct au fichier l'envoie au worker qui va traiter
la tâche.

## Options examinées

| Option | Verdict |
| --- | --- |
| Montage partagé (NFS/SMB) | Déjà géré, sans copie ; reste le premier choix quand il existe. |
| **HTTP direct worker → worker** | **Retenu.** Rapide, le central ne stocke rien, Range et SHA-256 simples, lié au bail. |
| Relais en flux par le central | **Retenu comme repli** quand aucune route directe n'existe. |
| WebDAV | Écarté : expose une arborescence, ignore baux et empreintes, ajoute un composant. |
| S3 / stockage objet | Écarté : infrastructure supplémentaire. |

## Décision

1. **Cascade de transport, du moins coûteux au plus coûteux :** (a) montage partagé commun, sans copie ;
   (b) HTTP direct du worker source vers le worker traitant ; (c) repli en flux par le central.
2. **Le worker traitant tire, le worker source sert.** Le worker traitant (repreneur) reste celui qui
   initie la connexion, comme tout worker ; le worker source (celui qui a l'accès direct) expose un
   petit **serveur de transfert en lecture seule**.
3. **Serveur de transfert du worker source** (port et adresse annoncée configurables, par exemple
   `WORKER_TRANSFER_PORT` et `WORKER_TRANSFER_URL`) : une seule route `GET` de lecture d'un fichier,
   `Range` pris en charge (reprise), flux sans copie, aucun listage de dossier, nombre de transferts
   simultanés borné. Il est facultatif : sans adresse annoncée, le worker n'est servi que par le repli.
   L'adresse annoncée est enregistrée avec le worker (enrôlement et heartbeat).
4. **Autorisation par billet opaque et court, validé par le central.** À l'attribution d'une tâche
   dont la source est sur un autre worker, le central remet au worker traitant un **billet** :
   adresse du worker source et secret aléatoire (256 bits, `opaque_token`). Seule l'empreinte SHA-256 du secret est
   stockée (`source_tickets`), avec le job, le worker source, le worker traitant, l'époque du bail et
   une expiration courte (10 minutes, assez pour une reprise par `Range`). Le worker source présente
   le secret au central (`POST /api/v1/worker/transfers/redeem`), qui répond seulement si le billet
   n'est pas expiré, lui est destiné et si le bail du worker traitant est toujours en cours à la même
   époque ; il reçoit alors ce qu'il doit servir (racine, nom, taille, SHA-256). Le secret voyage dans
   un en-tête, jamais dans l'URL, et n'est jamais journalisé. (Remplace l'idée initiale d'un HMAC
   signé avec une clé de worker : le central ne conserve que l'empreinte des clés de worker, et la
   validation par le central apporte la révocation et un point de contrôle unique.)
5. **Intégrité :** le worker traitant vérifie la taille et le SHA-256 déjà portés par le job
   (`source_sha256`, `source_bytes`) avant d'exécuter ; une empreinte invalide est une erreur
   `source_integrity`, sans effet sur la source. La source n'est jamais modifiée par ce transfert.
6. **Repli par le central, en flux et sans stockage :** si la route directe échoue (adresse absente,
   délai de connexion court dépassé, refus), le worker traitant demande la source au central
   (`GET /api/v1/jobs/{id}/source`). Le central demande au worker source, par long-polling comme le
   courtier d'exploration, de lui envoyer le fichier, et relaie les octets vers le worker traitant avec
   une mémoire tampon bornée. Les deux workers sont connectés en même temps ; en cas de coupure le
   transfert recommence (pas de reprise par `Range` dans cette version).
7. **Éligibilité d'un relais (précise l'ADR 0018) :** un worker est une cible possible s'il lit la
   source par montage, **ou** si le worker source est en ligne (route directe annoncée ou repli).
   La cible reste exclue si la source est inaccessible par les trois voies.
8. **Réseau :** l'adresse du worker source vient du central, donc de confiance, et doit être en
   `http` ou `https` sans identifiants. La politique SSRF de `net_policy` n'est **pas** appliquée : elle
   bloque les adresses privées, alors que les workers d'un même réseau local sont précisément la
   cible de la voie directe. Le TLS est facultatif (reverse proxy ou réseau privé), comme pour le central.
9. **Configuration d'un worker source :** `WORKER_TRANSFER_URL` (adresse annoncée, par exemple
   `http://nas.lan:9000`) et `WORKER_TRANSFER_PORT` (port d'écoute ; à défaut, le port de l'adresse).
   Les deux absents : voie directe désactivée, seul le repli est utilisé.

## Hors périmètre (à traiter ensuite)

- **Retour du résultat :** il revient au central comme aujourd'hui (`PUT /api/v1/jobs/{id}/result`).
  La publication d'un résultat vers un dossier hébergé par un autre worker (remplacement de
  l'original, écriture à distance) n'est pas couverte ; à vérifier avant l'implémentation et à traiter
  dans un ADR dédié si nécessaire.
- Limitation de débit, chiffrement applicatif du flux, compression, reprise du repli par `Range`.
- Worker intégré au central : il lit la source par la voie directe seulement (pas de repli par le
  central, qui est lui-même).
- Relais en cours de flux (fichier de travail déjà produit) : reste exclu (ADR 0018).

## Conséquences

- Nouveau composant côté worker : serveur de transfert en lecture seule (Axum, déjà dans le binaire).
- Migration `0032_source_transfer` : adresse de transfert annoncée par worker et table `source_tickets`.
- `claim` et le choix de cible (`worker_can_read_source`) tiennent compte de la voie directe et du repli.
- Tests natifs : signature et expiration du billet, refus d'un billet d'une autre époque de bail,
  lecture avec `Range`, empreinte corrompue rejetée, bascule vers le repli, refus de tout chemin
  hors du dossier de bibliothèque.

## Retour arrière

Ne pas annoncer d'adresse de transfert et désactiver la voie directe ramène au comportement actuel
(montage partagé ou source du central) ; le repli par le central reste utilisable seul.
