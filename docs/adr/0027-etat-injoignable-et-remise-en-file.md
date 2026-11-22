# 0027 — État injoignable et remise en file

Statut : accepté par l'utilisateur (22 nov. 2026). Central, workers et interface implémentés ; corrigé après
revue contradictoire (règle unique du bail, journal autorisé, code `lease_lost`).

Remplace l'[ADR 0012](0012-perimetre-perte-de-bail-et-versions-de-flows.md) point 2 (échec à la perte de
bail) et l'[ADR 0018](0018-relais-de-tache-par-bloc.md) point 5 dans ce qu'il en reprend. Tranche
l'incohérence avec la table des pannes de l'[ADR 0022](0022-artefacts-retenus-et-tires-en-direct.md)
(« tâche remise en file »). Précise l'[ADR 0026](0026-variables-flux-d-echec-et-sous-flux.md) §2.

## Contexte (v0.79.0)

- Bail de 120 s. `repository::claim` fait échouer les baux expirés, donc seulement quand un worker
  demande une tâche.
- Le worker arrête sa tâche au premier battement raté (`WorkerLoop::heartbeat` propage toute erreur).
- Un rapport final perdu (étapes, artefact, résultat, relais) fait échouer la tâche.
- Le flux d'échec peut s'exécuter deux fois.
- Un redémarrage de worker n'est pas détecté.
- La requête `result/ready` est annulée au central si le worker se déconnecte : une coupure pendant la
  publication perd la confirmation.

## Décision

1. Le central est le seul décideur ; les workers tirent leurs tâches.
2. Sans contact pendant 30 s, la tâche devient **injoignable**. Elle n'échoue pas, n'est donnée à
   personne et occupe toujours sa place. Le worker continue.
3. Passé le délai d'injoignabilité (réglage d'instance, 10 min par défaut, 1 min à 24 h), elle est
   remise en file dans la limite de `MAX_ATTEMPTS` (3) ; au-delà, échec `lease_expired` et flux d'échec.
4. Jamais deux titulaires pour une tâche.
5. Un accord du central précède toute action irréversible. Une action non idempotente commencée exclut
   la remise en file automatique.
6. Un redémarrage de worker ne concerne que les tâches qu'il exécutait.
7. Le balayage tourne en continu dans le central ; `claim` n'expire plus rien.
8. Le flux d'échec ne suit qu'un échec définitif, avec un seul exécutant autorisé.

Hors périmètre : relais de messages par un autre worker (refusé), plusieurs détenteurs de fichiers et
reprise (PR suivante), découpage parallèle d'un média.

## Modèle

`JobStatus` ne change pas (CHECK, anciens clients). « Injoignable » est `running` avec
`jobs.unreachable_since` non nul.

| Colonne | Sens |
| --- | --- |
| `leased_until` (existante) | Échéance du bail : dernier contact + 30 s + délai d'injoignabilité (120 s au protocole 0) |
| `lease_contact_at` | Dernier contact du titulaire (réservation, battement ou accord) |
| `unreachable_since` | Début de l'injoignabilité ; n'a de sens qu'en `running` |
| `lease_protocol` | 0 : ancien worker ; 1 : protocole de cet ADR |
| `lease_orphaned` | Processus disparu, publication encore à conclure : n'occupe plus de place |
| `free_releases` | Restitutions qui ont rendu leur tentative (3 au plus) |
| `ungated_lease` | Relayée sous un bail de protocole 0 : rien n'y a été inscrit, jamais de remise en file automatique |
| `workers.boot_id` | Dernier démarrage connu du worker |
| `source_publication_journal.authorized_at` | Accord donné à l'exécutant du remplacement |

**Règle unique :** un bail vaut tant que (worker, jeton, époque, `running`) est inscrit. L'échéance ne le
retire pas ; seule `end_lease` le fait. Aucune route ne compare plus `leased_until` à l'heure : après une
longue coupure, le premier appel reçu, quel qu'il soit, trouve son bail s'il n'a pas été repris. Un
nouveau réglage vaut au contact suivant de chaque tâche ; le battement le rend au worker
(`x-unreachable-timeout-seconds`), qui y règle sa patience.

### Machine à états

```text
            claim (époque+1, jeton neuf, tentative+1)
 queued ────────────────────────────────────────────► running joignable
   ▲                                                   │          ▲
   │                                          30 s sans contact   │ contact valide
   │                                                   ▼          │ (même jeton, même époque)
   │  échéance / redémarrage / arrêt propre      running injoignable
   ├───────────────────────────────────────────────────┤
   │  reprise permise (époque+1, jeton effacé)         │ reprise interdite
   │                                                   ▼
   └── relance manuelle ◄──────────────────────────  failed ──► flux d'échec (au plus une fois)
```

Le redémarrage et l'arrêt propre s'appliquent aussi à une tâche joignable.

### Fin involontaire d'un bail

Une seule fonction, `leases::end_lease`, dans une transaction `BEGIN IMMEDIATE`, décide dans cet ordre :

1. Journal de publication `in_progress` : un redémarrage laisse la tâche injoignable et orpheline, sa
   place rendue ; la publication est vérifiée aussitôt (intention jamais autorisée : écartée, tâche
   reprise ; remplacement prouvé : succès) ; sinon l'échéance conclut. Un arrêt propre est refusé. Une
   échéance donne `lease_expired` et une publication `uncertain`. Une publication que le central mène
   encore repousse l'échéance ; celle qui commence renouvelle le contact.
2. Action non idempotente au registre : échec explicite. Code noté avec l'accord du flux d'échec s'il
   existe, sinon `lease_expired`. Une étape `lease` en échec en donne la raison.
3. Bail d'ancien protocole, ou tâche relayée sous un tel bail (`ungated_lease`, effacé par la relance
   manuelle) : échec `lease_expired`, comme avant (voir Compatibilité).
4. `attempts >= MAX_ATTEMPTS` : échec `lease_expired`, relançable à la main (compte remis à zéro). Une tâche n'est donc jamais remise en file sans
   tentative restante (elle y resterait bloquée, `claim` exigeant `attempts < MAX_ATTEMPTS`).
5. Relais sans artefact vers un worker désigné hors ligne : échec `lease_expired` (la file de ce worker
   retiendrait la tâche sans fin). Un worker qui se signale (réservation, démarrage, worker intégré
   relancé avec le central) est en ligne.
6. Sinon remise en file : `queued`, titulaire, jeton et échéance effacés, époque + 1. L'arrêt propre (trois
   fois au plus) et un bail impossible à remettre rendent la tentative ; l'échéance et le redémarrage la consomment (une tâche qui tue son worker
   s'arrête après trois essais).

La priorité et la date de création sont conservées. N'importe quel worker admissible, l'ancien compris,
peut reprendre la tâche.

**Données de l'ancien bail**, dans la même transaction :

- artefacts et billets de la tâche supprimés ; l'ancien détenteur purge ses fichiers retenus
  (`artifact_retained` devient faux) ;
- relais par artefact : la tâche repart de la source (`job_relays`, `job_handoff_steps`, avertissement
  et cible effacés), comme une relance manuelle. Même helper pour les deux ;
- relais sans artefact : point de reprise et cible conservés, la source étant intacte ;
- `target_worker_id` choisi par l'utilisateur conservé : la tâche attend ce worker, son absence est visible
  (`awaiting_worker` dans `/jobs/leases`) ;
- étapes : celles de la tentative perdue restent lisibles en file et disparaissent à la réservation
  suivante, hors préfixe de remise ;
- demande de publication en mémoire : ses routes vérifient le bail, l'accord est refusé.

## Garantie : un seul titulaire

- `claim` attribue sous `BEGIN IMMEDIATE`, avec une époque supérieure et un jeton aléatoire neuf.
  SQLite n'admet qu'un écrivain : deux réservations ne peuvent pas prendre la même tâche.
- `end_lease` annule l'ancien bail **avant** que la tâche ne redevienne réservable.
- Chaque écriture du titulaire porte worker, jeton et époque, vérifiés au central : battement,
  progression, étapes, échantillon, artefact (déclaration, réception), billet, relais, échec, résultat,
  accord. Contrôle (`ensure_held`) et écriture sont dans **une** transaction : sans cela, une fin de bail
  entre les deux laisserait écrire l'ancien titulaire.
- Un ancien worker qui revient reçoit un refus définitif : il tue son processus, supprime son dossier de
  travail et ne publie rien.

**Ce qui n'est pas garanti.** Un ancien worker isolé peut *calculer* encore après la reprise. Deux calculs
peuvent donc coexister, jamais deux titulaires : le sien n'a aucun effet publié. Exceptions, bornées plus
bas : ce qu'un script ou un binaire générique écrit hors de son dossier de travail, et une action déjà
autorisée avant la coupure (elle exclut alors la remise en file automatique).

## Contact

### Côté worker

- Un battement toutes les 10 s. Réseau, délai, réponse sans corps Forgearr, 429, 401, 403, 503 : coupure,
  le travail continue. Un code métier connu (`result_undeliverable`, `source_changed`, `invalid_input`…)
  est un échec de traitement, déclaré ; un `internal_error` répété aussi, après un nombre borné d'essais.
- Refus définitif : **409** `lease_lost`, rendu seulement quand le bail ne correspond pas. Tout autre 409
  ou 404 (ancien central compris) est confirmé par un battement : accepté, une répétition puis un échec
  déclaré ; refusé, arrêt. Attente épuisée alors que les battements passent : échec **déclaré**.
- Appels de fin (billet, étapes, déclaration, relais, accord, résultat) : répétés tant que le central
  est injoignable, au plus délai + 60 s de marge. Les routes répétables sont rendues idempotentes :
  - `result/ready` : publication détachée de la connexion HTTP, inscrite dans `PublicationsInFlight` avant
    de relire bail et journal ; une répétition la rejoint, retrouve l'application locale déjà inscrite, ou
    constate le succès de ce bail. Journal sans publication en mémoire (central redémarré) : **jamais** de
    second exécutant, 503 `publication_pending` jusqu'à ce que la reprise conclue ;
  - `result/applied` : conclusion détachée de la connexion elle aussi ; succès déjà acquis = 204 ; `source_changed` inscrit l'échec et retire le journal dans
    une transaction, la répétition retrouve la même réponse ;
  - relais : une réponse perdue ne supprime plus le fichier retenu (le relais a peut-être réussi) ; la
    purge périodique tranche. La répétition reçoit un refus, le worker s'arrête sans échec déclaré.
- Un téléchargement interrompu par la coupure est repris au retour du contact. Un transfert direct déjà
  ouvert ne dépend pas du central.
- Attente épuisée : arrêt local, purge, sans échec déclaré ; le central tranche.
- La purge des fichiers `.tmp` et `job-*` n'a plus lieu à chaque reconnexion (elle détruirait le travail
  continué) mais au démarrage seulement, dossiers `<tâche>-<bail>` de l'exécution précédente compris.

### Côté central

Boucle `lease_sweeper` toutes les 5 s : marque injoignable, traite les échéances, notifie. Un contact
remet le compteur à zéro et efface `unreachable_since`. Un seul instant est lu, verrou d'écriture tenu.
Plus de 45 s d'heure murale depuis la fin du passage précédent, dans un sens ou dans l'autre (suspension,
saut d'horloge), **décalent** contacts et échéances de cet écart, sans dépasser ce que donnerait un contact
immédiat, et le passage s'arrête là : le temps déjà compté le reste. Une base lente ne décale rien. Chaque
fin de bail a sa transaction : une tâche illisible ne bloque pas les autres.

`ClaimRequest.active_jobs` (facultatif) liste les tâches tenues : celles que le central attribue à ce worker,
absentes de la liste et sans contact depuis 30 s, sont reprises (réponse de `claim` perdue). Une erreur du
central après la réservation rend le bail aussitôt, worker intégré compris : rien n'a été exécuté, la tâche
revient en file avec son relais (tentative rendue trois fois, puis consommée), enfin `lease_expired`,
relançable, si l'erreur persiste (clé absente du trousseau). Le worker reçoit l'erreur.

## Redémarrages

**Worker.** Un `boot_id` aléatoire par processus, optionnel, est envoyé **uniquement au premier
enregistrement** (répété à l'identique tant que la réponse manque). S'il diffère du `boot_id` connu, les
tâches `running` de ce worker sont relevées avec l'écriture du `boot_id`, puis traitées par `end_lease` une à
une. Ignoré (et journalisé) si la clé n'est pas celle du worker enregistré. Les
ré-enregistrements périodiques ne l'envoient pas : ils n'annulent rien. Absent (ancien worker) : aucun
effet, l'échéance s'applique. Les tâches en file ne bougent pas.

Deux processus de même nom restent une erreur de configuration : le second démarrage annule les baux du
premier, qui s'arrête à son battement suivant. La garantie tient, mais du travail est perdu.

**Arrêt propre.** Le worker cesse de réserver puis, par tâche : abandon, fin constatée du processus enfant
(et de son groupe), et seulement ensuite restitution
(`POST /api/v1/jobs/{id}/release`, 8 s au plus au total, avant les 10 s de Docker) et sort. Sans réponse,
le redémarrage ou l'échéance tranche.

**Central.** Avant d'écouter et avant toute reprise de publication :

1. tâches `running` : injoignables, compteur à zéro, échéance selon leur protocole ;
2. reprise des publications interrompues (existante, sur des baux redevenus valides) ;
3. tâches du worker intégré (`workers.serves_central = 1`) : `end_lease(WorkerRestarted)`, que le worker
   intégré soit encore activé ou non.

## Actions irréversibles

`POST /api/v1/jobs/{id}/irreversible/authorize` (identité du worker, bail, action, nœud, code d'erreur
pour le flux d'échec). Le central vérifie le bail et inscrit l'action dans **la même transaction**. Un
accord redemandé par le même bail est redonné : la réponse s'était perdue, l'action n'a pas commencé.
Injoignable : attente bornée. `lease_lost` : arrêt. `lease_expired_or_conflict` : le bail vaut, l'action est
exclue (publication en cours, flux d'échec déjà autorisé). `invalid_input` : identifiant de nœud vide ou de
plus de 128 caractères, jamais accordé sans pouvoir être inscrit.

La base est en `synchronous=FULL` : un accord, une attribution ou une intention validés survivent à une
coupure de courant.

| Action | Accord | Exclut la remise en file |
| --- | --- | --- |
| Remplacement de l'original (détenteur, worker local) | Routes `authorize` existantes | Oui, journal de publication → `uncertain` |
| Opération en place (`mkvpropedit`) | `in_place`, par bloc | Non : elle modifie la copie de travail, jamais l'original |
| Appel HTTP autre que GET, HEAD, OPTIONS (méthode calculée comprise) | `http_call`, par bloc | Oui |
| Appel HTTP GET, HEAD, OPTIONS seulement | Aucun | Non |
| Flux d'échec | `failure_flow` | Oui : l'échec est définitif |
| HTTP différé après publication | Aucun : exécuté par le central après succès | Sans objet |
| Dossier de sortie du central | Bail contrôlé ; nom unique par bail | Non : un fichier orphelin au pire |
| Script ou binaire générique en sortie | **Aucun** | Non |

Scripts et binaires génériques : ils sont censés n'écrire que dans leur dossier de travail. Un accord par
bloc ne prouverait rien et rendrait tout encodage non repris. **Limite :** un script qui agit à l'extérieur
(réseau, chemin absolu) peut agir deux fois après une remise en file ; utiliser un bloc HTTP.

Le registre vaut pour **toute la tâche**, pas seulement l'époque courante : après un relais, un bail
ultérieur perdu ferait rejouer l'appel du bail précédent. Choix prudent ; la relance manuelle
l'efface, l'administrateur assumant la répétition. Il n'efface pas un journal non résolu. Conséquence : un
flux qui appelle un webhook avant la publication n'est plus repris automatiquement après cet appel.

L'inscription précède l'action : un worker mort entre les deux donne un échec par excès de prudence. Le
journal de publication a le même biais, sauf intention jamais autorisée dont l'exécutant a disparu ou échoué :
elle est écartée sans risque (titulaire présent : il réannonce ou déclare un échec ordinaire ; disparu : la
tâche est reprise). Un accord sans intention inscrite est refusé. Une incertitude reste
`source_publication_uncertain` même échéance passée. Au redémarrage du central, un remplacement que le worker applique lui-même n'est jamais
déclaré incertain avant la fin de son bail.

Rejouer une source (`reprocess`) est refusé tant qu'un remplacement de cette source a une issue inconnue ;
l'administrateur lève l'incertitude en supprimant le résultat ou la tâche. Rejouer après un appel HTTP
reste permis : comme la relance manuelle, c'est un choix explicite.

## Flux d'échec

- Le worker demande `failure_flow` avec son code d'erreur. La transaction vérifie le bail, exige
  `failure_flow_state IS NULL` ou un accord déjà donné au même bail, refuse s'il existe une
  publication en cours (journal ou `PublicationsInFlight`), passe l'état à `done` et inscrit
  l'action. Le central ne l'exécutera plus : le déclencheur 0048 ignore un état non nul. **Il n'est pas
  remplacé** ; aucune modification de déclencheur n'est nécessaire.
- Réciproquement, un bail dont le flux d'échec est autorisé ne peut plus ni publier ni agir.
- Puis le worker exécute le flux, envoie ses étapes et l'échec. S'il perd son bail, la tâche échoue avec
  le code inscrit, sans remise en file ni seconde exécution. Les étapes perdues ne sont pas rejouées :
  **au plus une fois**, pas exactement une fois.
- Accord refusé : arrêt. Central injoignable jusqu'à la borne : le flux ne s'exécute pas.
- Échec sans accord (avant le flux, échéance, refus) : `pending`, exécuté par le central. La prise en
  charge exige désormais aussi `status = 'failed'` ; les étapes ne s'ajoutent que si l'époque de l'échec
  n'a pas changé ; `retry_job` remet l'état à NULL.
- Les blocs du flux d'échec sont couverts par son accord unique, sans accord par bloc.
- Une remise en file ne passe pas par `failed` : aucun déclenchement.

## Pannes

| Panne | Effet |
| --- | --- |
| Coupure < 30 s | Aucun |
| Coupure entre 30 s et l'échéance | Injoignable, puis rétablie ; rien n'est perdu |
| Coupure au-delà | Remise en file ou échec ; l'ancien worker s'arrête à son retour et purge |
| Retour après l'échéance, avant le balayage | Le battement rétablit le bail |
| Retour en même temps que le balayage | Un seul écrivain : bail rétabli, ou refus |
| Fin du travail pendant la coupure | Le worker attend et publie au retour |
| Coupure avant un accord | Attente bornée, aucune action |
| Coupure après un accord HTTP | L'action a pu avoir lieu : pas de remise en file |
| Coupure pendant un remplacement | Journal : succès prouvé ou `uncertain`, jamais de remise en file |
| Réponse de `result/ready` perdue | La publication continue ; la répétition retrouve l'issue |
| Réponse de `claim` perdue | Reprise après 30 s par `active_jobs`, sinon à l'échéance (tentative consommée) |
| Worker tué puis relancé | Remise en file immédiate à l'enregistrement |
| Worker tué, jamais relancé | Remise en file à l'échéance |
| Tâche qui tue son worker | Trois essais, puis échec |
| Central redémarré ou suspendu | Injoignable, compteur à zéro, rétablie au battement |
| Worker cible disparu | Choisi par l'utilisateur : attente visible ; relais sans artefact : échec |
| Détenteur de la source disparu | Règle existante ; un transfert échoué reste un échec de traitement |
| Annulation pendant l'injoignabilité | `canceled` ; le worker s'arrête à son retour |
| Deux workers au même nom | Un seul titulaire ; du travail est perdu |

Les échecs constatés par un worker joignable (outil, média, empreinte) restent définitifs.

## Compatibilité

- **Worker 0.79.0, central à jour.** `ClaimRequest.lease_protocol` absent → protocole 0. Il arrête
  toujours sa tâche à un battement raté et peut déclarer `transfer_failed` (inchangé). Il n'a demandé
  aucun accord et a pu appeler un webhook ou exécuter le flux d'échec : sa perte de bail reste un échec
  avec relance manuelle, jamais une remise en file. L'état injoignable et l'échéance s'appliquent. Le cas
  « flux d'échec deux fois » subsiste pour lui seul. Sa mise à jour se voit au premier `boot_id` ; ses
  tâches orphelines échouent aussitôt.
- **Worker à jour, central 0.79.0.** `Claim.lease_terms` absent → ni accord, ni reprise en attente longue
  (120 s, l'ancien bail), flux d'échec comme avant, `release` et `boot_id` ignorés. Le battement tolérant
  reste actif ; l'ancien central expire toujours à 120 s et le refus arrête le worker.
- `JobStatus` et `JobErrorCode` inchangés ; codes HTTP additifs `lease_lost` (409) et
  `publication_pending` (503). Trois événements SSE additifs, `job_unreachable`,
  `job_restored` et `job_requeued`, ignorés des anciens clients (écoute par nom).

## Migration `0049_lease_reachability.sql`

Additive, sans reconstruire `jobs` :

Colonnes du modèle ci-dessus, index partiel des tâches en cours et table `job_irreversible_actions` (clé
tâche, époque, action, nœud). Les intentions de publication antérieures sont tenues pour autorisées.

Tâches `running` au passage : protocole 0, `lease_contact_at` posé par le démarrage du central. Pas de
retour à une version antérieure sans restaurer la base (règle existante). Requêtes neuves ou modifiées
en SQLx runtime ; les macros inchangées restent.

## Plan de code

Un module par responsabilité. `Job` et `Worker` ne gagnent aucun champ (`query_as!`).

### Contrats (`shared`, `src/domain.rs`)

- `shared/src/jobs/lease.rs` (nouveau) : `UNREACHABLE_AFTER_SECONDS = 30`,
  `DEFAULT_UNREACHABLE_TIMEOUT_SECONDS = 600`, bornes 60 et 86 400, `LeaseProtocol { Legacy, Reachability }`
  (valeur numérique, inconnue → la plus récente connue), `IrreversibleAction` (serde snake_case,
  `as_str`, `blocks_requeue`), `LeaseReachability { Reachable, Unreachable }`,
  `JobLeaseInfo { job_id, reachability, unreachable_since, expires_at }`.
- `JobEventType` : `JobUnreachable`, `JobRestored`, `JobRequeued`.
- `Register.boot_id: Option<Uuid>` ; `ClaimRequest.lease_protocol: Option<i64>` ;
  `Claim.lease_terms: Option<LeaseTerms { unreachable_timeout_seconds: u64 }>` ;
  `AuthorizeAction { worker_id, lease_token, lease_epoch, action, node_id, error_code: Option<JobErrorCode> }`.
- `LEASE_SECONDS` ne sert plus qu'au repli du worker face à un ancien central.

### Central (implémenté)

- `repository/leases/` : `contact`, `sweep`, `restart`, `end` (`end_lease`, seule fin involontaire), `outcome`,
  `view`.
- `repository/irreversible_actions.rs` : accords et registre.
- `repository/source_publication_journal.rs` : intention répétable, accord durable, intention écartée.
- `lease_sweeper.rs`, `api/leases.rs`, `api/publications_in_flight.rs`.
- `repository::claim` reste le protocole 0 ; `claim_with_protocol` sert l'API. Le worker intégré y passe quand
  il demande ses accords.

### Worker

- `src/worker/contact.rs` : `classify(status, code) -> CentralReply`, `LeaseContact`.
- `src/worker/lease_calls.rs` : `TaskLease` (appels répétés, battement tolérant, accord, restitution, échec).
- `src/worker/held_tasks.rs` : tâches tenues (`active_jobs`) ; `src/worker/startup_purge.rs` : purge de démarrage.
- `src/supervision.rs` : groupe de processus de chaque outil ou script, tué en entier à l'abandon ; fin
  constatée avant restitution ; section critique (remplacement d'une source) menée à son terme.
- `src/worker.rs` : `execute` distingue `TaskStop { LeaseLost, Unreachable, Shutdown, Concluded }` (aucun échec
  déclaré) des erreurs de traitement ; `work` envoie `boot_id` au premier enregistrement seulement,
  attend les restitutions avant `abort_all`, ne purge qu'au démarrage.
- `src/worker/{handoff,local_apply,result_pull}.rs` : appels répétables, fichier retenu conservé sur
  issue ambiguë, `authorize_replacement` patiente.
- `src/local_worker.rs`, `src/local_worker/lease_gate.rs` : battement tolérant (`LeaseLost` seul définitif),
  accord direct en base, publication détachée, restitution à l'arrêt.

### Exécution

- `src/job_runner/action_gate.rs` (nouveau) : trait objet-sûr `ActionGate`, futur en boîte, implémenté par
  le worker distant et le worker intégré ; `halt(&self) -> Option<TaskStop>` mémorise le refus.
- `FlowEngine.action_gate: Option<&dyn ActionGate>` ; appel avant `Http` non sûr (hors différé) et avant
  `BinaryExecutionMode::InPlace` qui modifie. Absent : aperçus, flux d'échec, ancien central, tests.
- `src/flows/contract.rs` : `HttpRuntime::is_safe()` (toutes les méthodes littérales GET, HEAD, OPTIONS).
- `execute_claim_processing(…, gate: Option<&dyn ActionGate>)` : arrêt du gate → pas de flux d'échec ;
  sinon accord `failure_flow` avant `run_failure_flow`.

### Interface et documentation

- `client` : badge « Injoignable » et heure de remise en file (`/jobs/leases`), événements dans
  `jobs/model.rs`, réglage dans la page réseau/système.
- `docs/openapi.yaml`, `README.md` (baux), `docs/developpement.md` ; mentions croisées dans les ADR 0012,
  0018, 0022 et 0026.

## Plan de tests

- `tests/lease_reachability.rs` : 30 s → injoignable et place occupée ; battement → rétablie ; échéance →
  remise en file, époque et jeton neufs ; retour avant le balayage ; trois tentatives → échec et
  flux d'échec `pending` ; remise en file sans flux d'échec ; arrêt propre rend la tentative ; délai
  réglé ; démarrage du central ; worker intégré ; saut d'horloge ; `claim` n'expire plus.
- `tests/lease_single_holder.rs` : réservations concurrentes → une seule ; balayage et battement
  concurrents ; **chaque** écriture de l'ancien titulaire refusée (table des routes) ; nouveau titulaire
  intact ; annulation pendant l'injoignabilité.
- `tests/lease_requeue_cleanup.rs` : artefacts, billets, relais avec et sans artefact, cible, étapes.
- `tests/worker_boot.rs` : même démarrage, nouveau, absent, premier connu ; tâches en file intactes ;
  publication en cours différée ; registre bloquant.
- `tests/irreversible_actions.rs` : refus sans bail, idempotence, HTTP bloquant, `in_place` non bloquant,
  relais après HTTP, relance manuelle, étape explicative.
- `tests/failure_flow_pending.rs` : accord unique ; double demande même bail ; perte après accord → échec
  avec le code inscrit, aucune seconde exécution ; course avec une relance.
- `tests/lease_compatibility.rs` : sérialisations sans les champs neufs, protocole 0 → échec, `claim`
  ancien et neuf.
- `src/api/result_ready_tests.rs` : connexion coupée, répétition, issue retrouvée.
- Unitaires : `classify` (409 du central, 404 nu, 404 du central, 401, 5xx), `HttpRuntime::is_safe`,
  enums, bornes du réglage, `halt`, purge de démarrage, worker qui continue (serveur Axum factice), accord du
  worker intégré, opération en place, balayage notifié, publication orpheline.
- À adapter : `tests/job_repository.rs`, `tests/failure_flow_pending.rs`, `tests/hardening_api.rs`,
  `tests/source_publication_journal.rs`, `src/api/source_recovery.rs` (expiration par `sweep`, remise en
  file au lieu d'échec), littéraux `Claim { … }` et `FlowEngine { … }`.

Vérifications : GitHub CI seulement.

## Limites

- Calcul concurrent possible chez un ancien worker isolé ; effets externes d'un script générique non
  couverts.
- Une action autorisée peut partir tard (machine suspendue) : pas de remise en file, mais une relance
  manuelle entre-temps la doublerait. Aucun jeton de clôture n'est vérifiable par un service HTTP tiers.
- À l'abandon, un processus qui quitte son groupe (`setsid`) échappe à l'arrêt ; un groupe qui ne se vide pas
  2 s après `SIGKILL` (zombies sans parent) est tenu pour mort.
- Au plus une exécution du flux d'échec ; un arrêt peut l'interrompre.
- Excès de prudence : HTTP inscrit mais non parti, publication inscrite mais non commencée.
- Un central redémarré plus souvent que le délai repousse chaque fois l'échéance.
- Horodatages SQLite à la seconde ; un saut d'horloge de moins de 45 s n'est pas corrigé.
- Remplacement gelé entre sa dernière vérification et son `rename` : au-delà de 5 s d'horloge murale la source
  est revérifiée juste avant ; un gel dans cet ultime intervalle reste possible, et la
  source ne se rejoue pas avant que l'administrateur ait levé l'incertitude.
- Le moteur de base est unique : pas de consensus si deux centraux partagent une copie de la base.

## Alternatives écartées

- Valeur `unreachable` dans `JobStatus` : CHECK à reconstruire, anciens clients cassés.
- Remise en file dès 30 s : perd un long encodage à chaque coupure brève.
- Laisser `leased_until` à 30 s : toutes les vérifications d'écriture refuseraient un bail injoignable.
- `boot_id` à chaque enregistrement : deux processus de même nom s'annuleraient sans fin.
- Accord pour tout bloc : aucune tâche d'encodage ne serait reprise, sans protection réelle.
- Échec d'abord, flux d'échec ensuite sans bail : étapes et secrets à autoriser par un second jeton.
