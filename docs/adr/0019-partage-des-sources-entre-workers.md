# 0019 — Partage des sources entre workers : HTTP direct, repli par le central

Statut : proposé (19 nov. 2026), transport validé par l'utilisateur (HTTP direct avec repli par le
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
4. **Autorisation par billet court, négocié par le central.** À l'attribution d'une tâche dont la
   source est sur un autre worker, le central remet au worker traitant un **billet** : adresse du worker
   source, référence de la source (dossier de bibliothèque et nom, jamais un chemin libre), worker
   traitant, époque du bail, expiration courte (quelques minutes). Le billet est signé par HMAC-SHA256
   avec la clé propre du worker source, que ce dernier connaît et que le central connaît : le worker
   source le vérifie seul, sans rappeler le central. Il voyage dans un en-tête (`Authorization`), jamais
   dans l'URL, et n'est jamais journalisé. Il est invalide si le bail change d'époque.
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
8. **Réseau :** l'adresse du worker source vient du central, donc de confiance ; le worker traitant
   applique malgré tout la politique réseau existante (`net_policy`) et accepte `http` ou `https`
   seulement. Le TLS est facultatif (reverse proxy ou réseau privé), comme pour le central.

## Hors périmètre (à traiter ensuite)

- **Retour du résultat :** il revient au central comme aujourd'hui (`PUT /api/v1/jobs/{id}/result`).
  La publication d'un résultat vers un dossier hébergé par un autre worker (remplacement de
  l'original, écriture à distance) n'est pas couverte ; à vérifier avant l'implémentation et à traiter
  dans un ADR dédié si nécessaire.
- Limitation de débit, chiffrement applicatif du flux, compression, reprise du repli par `Range`.
- Relais en cours de flux (fichier de travail déjà produit) : reste exclu (ADR 0018).

## Conséquences

- Nouveau composant côté worker : serveur de transfert en lecture seule (Axum, déjà dans le binaire).
- Migration à venir : adresse de transfert annoncée par worker.
- `claim` et le choix de cible (`worker_can_read_source`) tiennent compte de la voie directe et du repli.
- Tests natifs : signature et expiration du billet, refus d'un billet d'une autre époque de bail,
  lecture avec `Range`, empreinte corrompue rejetée, bascule vers le repli, refus de tout chemin
  hors du dossier de bibliothèque.

## Retour arrière

Ne pas annoncer d'adresse de transfert et désactiver la voie directe ramène au comportement actuel
(montage partagé ou source du central) ; le repli par le central reste utilisable seul.
