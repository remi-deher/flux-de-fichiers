# 0020 — Blocs de transfert dans le flux : décision sur les octets détenus, transfert à tout moment

Statut : proposé (19 nov. 2026), orientation validée par l'utilisateur. Généralise l'[ADR 0018](0018-relais-de-tache-par-bloc.md)
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
4. **Transfert à tout moment, avec le fichier de travail.** Quand un fichier de travail existe, le
   nœud d'origine **le conserve** dans un dossier retenu (`handoff-<job>`) et le sert à la cible par le
   mécanisme de l'ADR 0019 (billet, voie directe, repli par le central). Le point de reprise comprend
   le nœud de reprise, le nom, la taille et le SHA-256 du fichier de travail, et les métadonnées. Le
   central enregistre le tout (extension de `job_relays`).
5. **Poignée de main de transfert** (inspirée de l'ADR 0016 d'`encoding-central`) : le nœud d'origine gèle
   l'exécution et libère son bail et son créneau de concurrence ; le central place la tâche ; le repreneur
   la prend, télécharge et vérifie le fichier de travail, puis **accuse réception** au central
   (`POST /api/v1/jobs/{id}/handoff/ack`) ; l'accusé autorise le nœud d'origine à purger le dossier
   retenu. Le repreneur **resonde** le fichier reçu avant de poursuivre, sans faire confiance aux
   métadonnées transmises.
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
