# 0020 — Blocs de transfert dans le flux : décision sur les octets détenus, transfert à tout moment

Statut : accepté, implémentation en cours (20 nov. 2026) ; les points 4 à 6 (remise via le central)
sont remplacés par l'[ADR 0022](0022-artefacts-retenus-et-tires-en-direct.md) (proposé ; ancien flux supprimé). Généralise l'[ADR 0018](0018-relais-de-tache-par-bloc.md)
(relais avant tout traitement) en s'appuyant sur le plan de données de l'[ADR 0019](0019-partage-des-sources-entre-workers.md).
Le traitement en parallèle d'un même média par plusieurs nœuds fait l'objet d'un chantier à part (voir plus bas).

## Contexte

Un nœud doit pouvoir laisser une tâche à un autre nœud selon les règles du flux : par exemple « si le
fichier est en H.265, le transférer à un nœud précis ou à un nœud capable ». À terme, un même média
pourrait être réparti (vidéo sur un nœud, audio sur un autre).

L'idée d'un **scan précoce** qui fixerait les métadonnées puis le placement par le central a été écartée :
ces métadonnées deviennent fausses si le fichier est remplacé entre le scan et le traitement, et il
faudrait les revalider à chaque fois. L'ADR 0018 posait déjà le bon principe, avec une limite : le relais
n'était permis qu'avant tout traitement.

## Décision

1. **Le flux décide, sur les octets que le nœud détient.** Le nœud sonde la copie qu'il traite (le
   snapshot vérifié par taille et SHA-256), évalue les conditions du flux, puis exécute un bloc de
   transfert si le flux le demande. Le central ne décide jamais du placement à partir de métadonnées
   antérieures. Le scan de bibliothèque reste une **découverte** de fichiers (création des tâches),
   jamais une source de décision.
2. **Filet de sécurité inchangé :** taille et SHA-256 du job sont revérifiés à chaque téléchargement, et
   la publication est refusée (`Conflict`) si la source a changé. Un fichier remplacé en cours de route
   échoue proprement sans écraser le nouveau fichier.
3. **Un bloc de transfert général.** Le bloc « Relayer la tâche » devient le bloc **« Transférer la
   tâche »** (identifiant et alias `relay` conservés). Cibles : un nœud précis, ou un nœud capable. Il
   reste un bloc natif (runtime `primitive`) à une seule sortie « Reprise ». Le bloc « Capacité du
   worker » reste l'aiguillage `capable` / `incapable`.
4. **Transfert à tout moment, avec le fichier de travail.** Le nœud d'origine conserve son dossier
   de travail pendant la remise. Il envoie au central une copie temporaire, limitée et vérifiée par
   taille et SHA-256 ; la cible la télécharge avec un jeton individuel. Le central conserve le nom,
   l'empreinte, la taille et le nœud de reprise dans `job_handoffs`.
5. **Poignée de main de transfert** : le nœud d'origine gèle l'exécution et renouvelle son bail.
   Le repreneur réserve l'offre, télécharge et vérifie le fichier, puis **accuse réception** au central
   (`POST /api/v1/jobs/{id}/handoff/ack`). L'accusé libère le bail et autorise l'origine à purger son
   dossier ; il peut être répété sans déplacer deux fois la tâche. Le repreneur **resonde** le fichier
   reçu avant de poursuivre avec le flux figé.
   Les avertissements et le journal des étapes déjà exécutées sont conservés à la reprise.
6. **Pannes :** si le nœud d'origine disparaît avant l'accusé, la tâche attend sa reconnexion pendant une
   durée bornée (constante nommée), puis échoue avec `transfer_failed` ; jamais de perte silencieuse. Le
   dossier retenu a une durée de vie maximale et est purgé au démarrage s'il est périmé (la purge actuelle
   des dossiers `job-*` ne doit pas le toucher). La source d'origine n'est jamais modifiée par un
   transfert.
7. **Opérations en place :** si le flux a déjà modifié la source en place (par exemple édition de
   métadonnées Matroska), un transfert à ce stade est refusé avec un message explicite : transférer un
   fichier modifié en place exposerait la source à un état intermédiaire.
8. **Capacités matérielles et logicielles.** Aujourd'hui seul le matériel est déclaré, donc un nœud sans
   GPU n'a aucune capacité. Le worker déclare aussi ses **encodeurs logiciels**, lus dans l'outil FFmpeg
   installé par les plugins (liste des encodeurs, par exemple `libx265`), en plus des sondes matérielles
   de l'ADR 0018. `EncodeAcceleration` gagne la valeur `cpu`. Une exigence sans accélérateur accepte
   n'importe lequel ; elle peut en imposer un. Le matériel confirmé par une sonde reste distinct d'un
   simple encodeur « compilé » dans FFmpeg.
9. **Bornes inchangées :** nombre maximal de relais par tâche, cible exclue si elle est le nœud d'origine,
   échec visible `no_capable_worker` sans cible éligible. Les cibles possibles seront bornées par les
   modes d'accès de la bibliothèque (ADR 0021) ; en attendant, la règle de l'ADR 0019 s'applique.

## Traitement en parallèle : chantier à part

Répartir un même média (vidéo sur un nœud, audio sur un autre) change le modèle : un job suit
aujourd'hui un seul fichier de travail. Il demandera, dans un ADR dédié : des sous-tâches avec leur bail,
des fichiers intermédiaires échangés entre nœuds, un bloc « Joindre » qui attend toutes les branches, une
règle d'échec (une branche en échec annule-t-elle les autres), et une publication finale atomique. Cet
ADR n'est pas implémenté ici ; le bloc de transfert en reste la brique.

## Conséquences

- Migration à venir : extension de `job_relays` (empreinte, taille et nom du fichier de travail, nœud
  d'origine, état d'accusé) et dossier retenu côté worker.
- Le billet de l'ADR 0019 peut désigner un fichier de travail retenu, et plus seulement la source.
- Nouveau point d'API : `POST /api/v1/jobs/{id}/handoff/ack`.
- Le moteur autorise le bloc de transfert après des étapes, sous les conditions des points 6 et 7.
- Tests natifs : transfert avec fichier de travail, empreinte corrompue rejetée, accusé et purge,
  origine absente (échec borné), refus après opération en place, capacité logicielle, boucle de relais.

## Retour arrière

Limiter le bloc au transfert avant tout traitement (comportement de l'ADR 0018) et ignorer les capacités
logicielles rétablit l'état précédent ; les colonnes ajoutées resteraient inertes.
