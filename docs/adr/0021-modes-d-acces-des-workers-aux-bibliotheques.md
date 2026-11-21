# 0021 — Modes d'accès des workers aux bibliothèques

Statut : accepté (21 nov. 2026), implémenté. Orientation et choix des points 4 et 8 confirmés par
l'utilisateur. Constaté dans le code : table `library_transfer_workers` (migration 0033),
`transfer_workers` dans l'API, le client et les outils MCP, attribution par appartenance dans `claim`,
détenteur choisi par `source_key` (`repository/source_tickets.rs`), journal de publication durable
(migration 0034, `source_publication.rs`) et échec `source_changed`. Précise les
[ADR 0018](0018-relais-de-tache-par-bloc.md), [0019](0019-partage-des-sources-entre-workers.md) et
[0020](0020-blocs-de-transfert-dans-le-flux.md).

## Contexte

Une bibliothèque est aujourd'hui une liste de **dossiers** : un worker (ou la centrale, identifiée par un
UUID nul), un chemin, un accès `read` ou `read_write`, et une `source_key` qui désigne le même dossier
logique monté par plusieurs workers (`library_folders`). Un scan crée les tâches ; la vue
`verified_job_source_mounts` dit quels workers voient chaque fichier ; `claim` ne donne la tâche qu'à ces
workers. Il n'existe donc que deux situations : accès direct, ou pas de dossier.

L'utilisateur veut trois situations : des workers à **accès direct** (qui peuvent aussi servir les fichiers
aux autres), des workers **sans accès direct mais qui traitent** (les fichiers leur sont transférés), et
des workers **sans aucun accès**.

Défaut constaté : pour faire fonctionner le relais, `claim` a été ouvert à toute tâche relayée
(`job_relays` existe). Un worker totalement étranger à la bibliothèque peut donc reprendre un relais.

## Décision

1. **Trois modes par couple (bibliothèque, worker).**
   - **Accès direct** : le worker a au moins un dossier déclaré dans la bibliothèque (chemin, accès,
     `source_key`). C'est le modèle actuel, inchangé.
   - **Par transfert** : le worker n'a pas de dossier ; il traite les tâches de la bibliothèque et reçoit
     les sources d'un worker à accès direct (ADR 0019), puis renvoie son résultat au central.
   - **Aucun accès** : tout worker absent des deux listes ; il ne prend jamais une tâche de cette
     bibliothèque et n'est jamais cible d'un transfert pour elle.
2. **Stockage.** `library_folders` ne change pas. Une nouvelle table `library_transfer_workers
   (library_id, worker_id)` liste les workers « par transfert » (suppression en cascade). Un worker ne peut
   pas être à la fois direct et par transfert pour la même bibliothèque : le mode direct prime et
   l'enregistrement refuse le doublon.
3. **Le worker intégré au central est un nœud comme les autres.** Il est « direct » quand la bibliothèque
   a un dossier de la centrale (UUID nul), et peut être « par transfert » sinon. Aucun cas particulier.
4. **Attribution (`claim`).** Une tâche de bibliothèque est attribuable à un
   worker s'il a un montage vérifié (comportement actuel) **ou** s'il est « par transfert » dans cette
   bibliothèque, y compris pour les tâches neuves, pas seulement les relais. Cela permet de dédier un
   worker de calcul à une bibliothèque hébergée ailleurs. L'ouverture « tout relais » de l'ADR 0018 est
   **supprimée** et remplacée par cette appartenance. Les tâches sans bibliothèque (dépôt au central) ne
   changent pas.
5. **Cibles d'un transfert ou d'un relais (ADR 0018 et 0020).** L'ensemble éligible est l'union des
   workers directs et par transfert de la bibliothèque de la tâche. Une cible précise hors de cet ensemble
   échoue avec `no_capable_worker`. Pour une tâche sans bibliothèque, la règle actuelle s'applique (source
   lisible par la cible).
6. **Détenteur de la source.** Le billet et le repli de l'ADR 0019 choisissent un worker **direct en
   ligne** parmi ceux qui partagent la même `source_key` (donc pas seulement celui qui a scanné), de
   préférence avec une adresse de transfert annoncée. Si deux workers montent le même dossier, la panne de
   l'un ne bloque rien.
7. **Scan.** Seuls les workers à accès direct (et la centrale) scannent une bibliothèque ; les workers par
   transfert ne scannent jamais. Le scan reste une découverte de fichiers (ADR 0020).
8. **Politique de sortie.** `OutputDir` conserve le résultat dans le dossier de sortie du central.
   `ReplaceWithBackup` et `ReplaceInPlace` sont également disponibles aux workers par transfert : le
   résultat est remis à un détenteur inscriptible de la source, ou publié par le central si celui-ci
   possède un montage inscriptible. Le détenteur vérifie la taille et le SHA-256 de la source et du
   résultat ; une source modifiée provoque l'échec explicite `source_changed`. Le central attend la
   réponse du détenteur avant de déclarer la tâche réussie. Un journal durable conserve la référence
   du résultat pendant le remplacement. Si une interruption rend son issue incertaine, la tâche échoue
   explicitement et le résultat reste téléchargeable ; une perte de bail conserve `lease_expired`.
   Au redémarrage, le central peut confirmer la réussite d'après les empreintes du résultat, du fichier
   publié et de la sauvegarde requise, seulement si le bail est encore valide. Il vérifie son propre
   montage inscriptible ou demande un constat en lecture seule à un détenteur distant autorisé, via un
   canal distinct de la publication. Sans preuve disponible, il conserve l'état incertain pour examen.
   La relance est suspendue tant que cette publication n'a pas été examinée et que le résultat conservé
   n'a pas été supprimé explicitement.
9. **API.** `transfer_workers: Vec<Uuid>` s'ajoute à `Library` et `LibraryWrite`, avec valeur par défaut
   (rien ne casse). Validation : workers existants, sans doublon avec les workers directs.
10. **Interface.** Le formulaire de bibliothèque présente **une liste de workers avec un mode par worker** :
    « Accès direct » (avec ses dossiers) ou « Par transfert ». Des avertissements : aucun worker direct
    n'annonce d'adresse de transfert (repli par le central). La fiche d'un worker affiche ses
    bibliothèques et son mode pour chacune.

## Hors périmètre

- Verrou global empêchant des traitements concurrents sur un même fichier source : chantier à part.
- Traitement en parallèle d'un même média (ADR 0020) et haute disponibilité.

## Conséquences

- Migration à venir : table `library_transfer_workers`.
- `claim` : le filtre de source remplace la clause `job_relays` par l'appartenance « par transfert » ;
  le choix de cible de `relay_job` et la sélection du détenteur (`attach_source_ticket`) utilisent
  l'ensemble de la bibliothèque et les workers partageant la `source_key`.
- Client et API des bibliothèques, outils MCP de bibliothèque et fiche worker à adapter.
- Tests natifs : worker hors bibliothèque jamais attribué ni cible, worker par transfert attribué, refus du
  doublon direct et transfert, remplacement avec workers par transfert, détenteur de secours en
  cas de panne d'un montage partagé, worker intégré en mode direct et par transfert.

## Retour arrière

Vider `library_transfer_workers` ramène au modèle actuel (accès direct seulement). Rétablir l'ouverture
« tout relais » dans `claim` n'est pas souhaitable : elle contredit le mode « aucun accès ».
