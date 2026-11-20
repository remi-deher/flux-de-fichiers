# 0022 — Artefacts retenus et tirés en direct : le central ne transporte aucun octet

Statut : proposé (20 nov. 2026), principe validé par l'utilisateur en session. Remplace le repli par
le central de l'[ADR 0019](0019-partage-des-sources-entre-workers.md) (point 6) et la remise via le
central de l'[ADR 0020](0020-blocs-de-transfert-dans-le-flux.md) (points 4 à 6). Précise l'[ADR 0021](0021-modes-d-acces-des-workers-aux-bibliotheques.md).

## Contexte

Le transfert de fichiers entre workers repose aujourd'hui sur quatre mécanismes distincts :
billet et serveur de transfert (voie directe), courtier de repli par le central, remise avec
téléversement vers le central (`job_handoffs`), et publication du résultat par le central
(`PUT /result`, courtier de publication). Chacun a ses états, ses jetons, ses délais et ses pannes.
L'audit du 20 nov. 2026 y relève, entre autres : un TTL de 600 s qui couvre toute la remise, une
offre réservée jamais ré-offerte, un staging central sans quota, et du code de jeton dupliqué.

Décision utilisateur : les workers s'échangent les fichiers directement ; le central n'en
transporte aucun.

## Décision

1. **Plan de données entre workers uniquement.** Le central ne garde que les métadonnées, les baux,
   les billets et l'orchestration. Disparaissent : le courtier de repli (`source_relay`), le
   téléversement et le téléchargement de remise, le staging central, `PUT /result` et le courtier de
   publication.
2. **Un artefact** est un fichier décrit par un nom, une taille, un SHA-256 et un **détenteur**
   (le worker qui l'a sur son disque). Il reste chez son détenteur, servi en lecture seule par le
   serveur de transfert de l'ADR 0019 (`Range`, billet). La même mécanique sert la source, un fichier
   de travail et un résultat. Le billet désigne un artefact, un détenteur, un preneur et une époque
   de bail ; sa validation par le central est inchangée.
3. **Le preneur tire toujours.** Aucun worker n'écrit sur le disque d'un autre. Un worker qui reçoit
   un artefact le tire chez son détenteur, vérifie taille et SHA-256, puis agit.
4. **Détenteur de la bibliothèque (`library_holder`).** Le job porte, du début à la fin, le worker
   qui détient la bibliothèque. C'est la destination du résultat, quel que soit le nombre de
   workers traversés.

## Cycle d'une tâche transférée

1. A (détenteur de la bibliothèque) détient le fichier, sonde les octets et exécute les premiers
   blocs du flux (ADR 0020, point 1).
2. Le bloc de transfert demande une capacité (par exemple H.265). Le central choisit B et remet la
   tâche en file avec `resume_node_id`, l'artefact d'entrée (A détient) et l'exigence de capacité.
3. B prend la tâche par le `claim` normal, tire l'artefact chez A avec un billet, le vérifie et
   exécute le flux à partir de `resume_node_id`.
4. B retient le résultat comme artefact et annonce au central : nom, taille, SHA-256, avertissements.
5. A tire le résultat chez B, le vérifie, applique la politique de sortie de la bibliothèque
   (point commit, ci-dessous), puis informe le central. Le central passe le job en `Succeeded` et B
   purge son résultat.

Si A est lui-même capable, ou si B monte la même source et la même sortie (ADR 0021), aucun
transfert n'a lieu.

## Remise entre étapes

Elle devient la même opération, répétable : le worker courant (B) retient son fichier de travail
comme artefact, la tâche est remise en file avec `resume_node_id`, l'artefact d'entrée et
l'exigence ; le repreneur (C) la prend par le `claim` normal et tire l'artefact chez B. Le résultat
final revient toujours à A (point 4). Le détenteur purge l'artefact dès que le repreneur l'a vérifié.
Conséquences sur le modèle :

- plus de table `job_handoffs`, ni de `claim_handoff`, de jeton de réception ou de `/handoff/ack` :
  l'état tient dans le job (artefact d'entrée, détenteur, étape de reprise) ;
- les étapes restent dans `job_steps` : plus de copie `job_handoff_steps` ;
- les bornes de l'ADR 0020 point 9 restent (nombre maximal de remises, cible différente de l'origine,
  `no_capable_worker`) ; le refus après opération en place (point 7) aussi.

## Point de commit : l'application de la politique de sortie par A

Ordre imposé chez A : (1) vérifier taille et SHA-256 du résultat tiré ; (2) vérifier que la source
n'a pas changé (`SourceChanged` sinon, sans rien remplacer) ; (3) appliquer la politique de sortie ;
(4) informer le central ; (5) le central confirme, B purge.

L'étape 3 est **idempotente** : si l'accusé se perd après le remplacement de l'original, la
répétition ne retraite pas le fichier. L'état « publication incertaine » existant reste le garde-fou
quand le résultat du remplacement n'est pas connu.

## Pannes

| Panne | Effet |
| --- | --- |
| B tombe avant la fin du flux | Bail expiré, tâche remise en file ; l'original de A est intact. |
| B tombe avant que A ait tiré le résultat | A n'a rien remplacé ; la tâche repart de l'original (échec borné `transfer_failed` si plus aucune cible). |
| A tombe avant le point de commit | B conserve le résultat jusqu'à une durée de vie maximale (constante nommée), A le tire à sa reconnexion. |
| Détenteur d'un fichier de travail disparu (remise B vers C) | Les fichiers retenus survivent au redémarrage (dossier du worker) ; passé la durée bornée, la tâche repart de la source d'origine chez A. |
| Accusé perdu après remplacement | Répétition idempotente (point commit). |

## Joignabilité

Un worker qui retient un artefact doit annoncer une adresse de transfert (ADR 0019). Sans adresse,
un worker ne peut traiter que ce qu'il lit et écrit par montage (ADR 0021). La règle d'éligibilité
d'une cible devient : montage de la source et de la sortie, **ou** adresse joignable. Elle remplace
le repli par le central.

## Worker intégré

Le worker hébergé par le central est un worker ordinaire dans le même conteneur : enrôlé comme les
autres, avec son dossier de travail et son propre serveur de transfert. Plus aucune requête SQL ne
nomme `INTEGRATED_WORKER_NAME` ; ses accès aux bibliothèques du central passent par les modes
d'accès de l'ADR 0021. Le central ne sert plus de fichier lui-même.

## Corrections de l'audit incluses

Contrôle d'identité du worker sur `redeem` ; permis de transfert pris après validation du billet ;
billet ré-émissible si le téléchargement démarre tard ; reprise par `Range` côté preneur ; un seul
module de secret opaque et d'empreinte ; constantes nommées pour tous les délais.

## Hors périmètre

- Traitement en parallèle d'un même média (ADR 0020) : l'artefact en est la brique, rien n'est
  construit ici.
- Téléchargement d'une sortie depuis l'interface (`GET /api/v1/jobs/{id}/output`) : la sortie vit
  chez le détenteur de la bibliothèque ; lien à billet court ou suppression de la route, à trancher.
- Chiffrement applicatif, compression, limitation de débit.

## Plan d'implémentation par étapes

Chaque étape est poussée seule et validée par la CI avant la suivante.

1. **Registre d'artefacts et billets d'artefact** (fait) : table `job_artifacts` (détenteur, racine,
   nom, SHA-256, taille), déclaration par le détenteur du bail (`PUT /api/v1/jobs/{id}/artifacts/{kind}`),
   billets qui désignent la source ou un artefact, `redeem` qui rend la racine et le nom de l'artefact.
   Additif : aucun flux existant ne change.
2. Remise entre étapes par artefact, qui remplace `job_handoffs`, en deux pushes :
   - **2a (fait) — côté central** : `relay_job` relaie un fichier de travail déclaré (le bail est libéré
     tout de suite, les étapes et l'avertissement sont conservés, le repreneur n'a pas besoin de joindre
     la source) ; le `claim` du repreneur porte le fichier de travail et un billet d'artefact ;
     `POST /api/v1/jobs/{id}/artifacts/working/received` libère l'artefact ; sans artefact, une reprise
     repart de la source. L'ancien flux `job_handoffs` reste en place.
   - **2b-i (fait) — côté worker** : le worker distant retient son fichier de travail (nom
     `handoff-<tâche>-<époque>` avec l'extension d'origine), le déclare, relaie la tâche et libère son
     bail ; le repreneur (worker distant ou intégré) le tire en direct, le vérifie et confirme la
     réception ; le détenteur purge les copies que `GET .../artifacts/working/retained` ne réclame plus.
     Limite transitoire : le worker intégré reçoit mais ne relaie pas encore, faute de serveur de
     transfert (étape 5).
   - **2b-ii (fait) — suppression de l'ancien flux** : table `job_handoffs` (migration 0038), routes
     `/handoff*` et `/worker/handoffs/*`, code central (téléversement, staging, accusé, nettoyage) et tests
     associés. `GET .../artifacts/working/retained` ne réclame plus le fichier d'une tâche terminée.
3. Retour du résultat tiré par le détenteur de la bibliothèque, sans copie centrale (décision
   utilisateur du 20 nov. 2026 : le worker intégré est le détenteur des bibliothèques à dossier de
   sortie du central), en pushes courts :
   - **3a (fait) — côté central, détenteur distant** : `POST /api/v1/jobs/{id}/result/ready` publie un
     résultat retenu chez le worker qui traite ; le détenteur de la source tire le fichier avec un
     billet d'artefact (`SourcePublicationTask.result_ticket`), applique la politique, puis la tâche
     réussit sans fichier au central. Les autres cas répondent `pulled: false` et gardent l'envoi au
     central.
   - **3b (fait) — workers distants** : le worker qui traite retient son résultat
     (`result-<tâche>-<époque>`), le déclare et appelle `result/ready` (repli sur l'envoi au central si
     `pulled: false` ou sans adresse de transfert) ; le détenteur distant le tire chez lui avec reprise
     par `Range` et billet ré-émissible (`POST .../source-publications/{id}/ticket`) ; la purge des
     fichiers retenus passe par `GET /api/v1/jobs/{id}/retained/{kind}`. Le détenteur qui traite lui-même
     applique encore via le central (raccourci local à venir).
   - **3c-i (fait) — worker intégré, sources tenues par le central** : le worker intégré (identité
     publiée dans `AppState.local_worker_id`) reçoit la demande par le courtier en mémoire, tire le
     résultat chez le worker qui le retient (billet d'artefact, reprise par `Range`), applique la
     politique de remplacement sur la source du central et répond ; sans worker intégré actif, l'envoi au
     central reste le chemin.
   - **3c-ii (fait) — dossier de sortie du central** : pour une bibliothèque `OutputDir` ou une tâche
     sans bibliothèque, le worker intégré tire le résultat et l'installe dans `output/` sous le nom
     `<tâche>-<bail>.<extension>` (`storage::output_file_name`), sans journal de publication : un échec
     retire le fichier installé. Sans worker intégré actif, l'envoi au central reste le chemin.
   - **3d (fait) — journal et reprise sans copie centrale** : pour un résultat remplacé chez le
     détenteur, le journal garde l'empreinte, la taille et l'extension (migration 0039,
     `begin_pulled_publication`) et `output_name` reste vide, de même que celui de la tâche ; la reprise
     après redémarrage prouve le remplacement avec ces seules données (source du central, ou inspection
     en lecture seule chez un détenteur distant) sans relire de copie centrale. Le fichier retenu chez le
     worker qui traite n'est pas encore purgé quand la publication reste incertaine (étape 3e).
   - **3e-i et 3e-ii (faits) — le détenteur qui traite applique lui-même** : quand le worker qui
     traite détient, sur son montage, la source d'une bibliothèque à politique de remplacement que le
     central ne voit pas, `result/ready` (avec `can_apply_locally`) inscrit le remplacement au journal et
     répond `apply_locally` ; le worker remplace la source (`result/authorize` juste avant, puis
     `result/applied` avec l'issue) et le central conclut. Rien ne transite par le central.
   - **3e-iii (fait) — plus aucun octet de résultat vers le central** : `PUT /result` et l'envoi du
     worker sont supprimés (route, handler, repli, documentation OpenAPI). `result/ready` ne répond plus
     `pulled` : soit le détenteur a tiré et appliqué le résultat, soit `apply_locally`, soit l'erreur
     explicite `result_undeliverable` (502) quand aucun détenteur actif ou aucune adresse de transfert ne
     permet de le recevoir (worker intégré désactivé, worker qui traite sans adresse) ; le résultat reste
     alors retenu et la tâche échoue. Un worker sans adresse peut encore appliquer lui-même. Restent
     jusqu'aux étapes 4 et 5 : le worker intégré qui traite (`publish_staged`, staging local) et la
     lecture `staged` pour ses résultats. La validation post-traitement se fait déjà chez le worker qui
     traite (`binary_runner`).
4. Suppression du repli par le central, de `PUT /result` et des routes de remise, en pushes courts :
   - **4a (fait) — plus de lecture de source en flux par le central** : le courtier `source_relay`,
     ses routes `/worker/source-relay/*`, la réponse du worker détenteur et `SourceRelayTask` sont
     supprimés. `GET /jobs/{id}/source` ne sert plus que les sources que le central voit lui-même ; une
     source détenue par un autre worker répond `source_unreachable` (502) et se lit chez lui avec un
     billet.
   - **4b (fait) — règle d'éligibilité à la prise d'une tâche** : `claim` et le choix de cible d'un
     relais exigent que la source soit lisible par le worker (`source_readable_sql`) : vue par le
     central, montée par lui, ou tenue par un détenteur en ligne qui annonce une adresse de transfert.
     Un relais par artefact s'en passe (il lit le fichier de travail chez son détenteur). Un détenteur
     sans adresse ne sert donc plus personne d'autre que lui.
5. Worker intégré traité comme un worker ordinaire.

## Conséquences

- Installation neuve uniquement (ADR 0012) : pas de migration de données ; une migration supprime
  `job_handoffs`, `job_handoff_steps` et les colonnes devenues inutiles, et ajoute l'artefact d'entrée
  et le `library_holder` au job.
- Code supprimé : `source_relay.rs`, `source_publication_broker.rs`, `api/handoffs.rs`
  (téléversement, téléchargement, accusé, nettoyage), `PUT /result`, cas particuliers du worker
  intégré. Code réutilisé : serveur de transfert, billets, `save_verified`, politique de sortie.
- Tests natifs : billet pour un artefact de travail et pour un résultat ; reprise par `Range` ;
  idempotence du point de commit ; B qui tombe avant et après le tirage ; remise B vers C avec retour
  chez A ; refus d'un billet d'une autre époque ; worker sans adresse exclu des cibles non montées ;
  worker intégré sans cas particulier.

## Retour arrière

Aucun code n'est modifié par cet ADR. Tant que l'implémentation n'est pas faite, les ADR 0019 et 0020
restent la référence ; un retour arrière après implémentation suppose de rétablir le courtier de
repli et la remise via le central.
