# 0021 — Modes d'accès des workers aux bibliothèques

Statut : proposé (19 nov. 2026). Orientation validée par l'utilisateur : une bibliothèque porte un flux et
des workers qui ont ou non un accès direct aux fichiers mais qui peuvent traiter les tâches. Les deux
choix détaillés ci-dessous (points 4 et 8) sont **confirmés par l'utilisateur**. Précise les
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
8. **Politique de sortie.** `OutputDir` (défaut) fonctionne avec les workers par
   transfert : le résultat va au dossier de sortie du central. Les politiques `ReplaceWithBackup` et
   `ReplaceInPlace` exigent d'écrire chez le worker qui détient la source, ce que personne ne sait faire à
   distance. Tant que le retour du résultat n'est pas implémenté (chantier à part), l'enregistrement d'une
   bibliothèque **refuse** de combiner une politique de remplacement avec des workers par transfert, et
   `claim` n'attribue pas à un worker par transfert une tâche d'une bibliothèque en remplacement.
9. **API.** `transfer_workers: Vec<Uuid>` s'ajoute à `Library` et `LibraryWrite`, avec valeur par défaut
   (rien ne casse). Validation : workers existants, sans doublon avec les workers directs, et règle du
   point 8.
10. **Interface.** Le formulaire de bibliothèque présente **une liste de workers avec un mode par worker** :
    « Accès direct » (avec ses dossiers) ou « Par transfert ». Des avertissements : aucun worker direct
    n'annonce d'adresse de transfert (repli par le central) ; politique de remplacement incompatible avec
    le mode par transfert. La fiche d'un worker affiche ses bibliothèques et son mode pour chacune.

## Hors périmètre

- Retour du résultat vers un dossier hébergé par un autre worker (remplacement de l'original,
  écriture atomique, copie `.bak`, verrou contre deux traitements du même fichier) : chantier à part.
- Traitement en parallèle d'un même média (ADR 0020) et haute disponibilité.

## Conséquences

- Migration à venir : table `library_transfer_workers`.
- `claim` : le filtre de source remplace la clause `job_relays` par l'appartenance « par transfert » ;
  le choix de cible de `relay_job` et la sélection du détenteur (`attach_source_ticket`) utilisent
  l'ensemble de la bibliothèque et les workers partageant la `source_key`.
- Client et API des bibliothèques, outils MCP de bibliothèque et fiche worker à adapter.
- Tests natifs : worker hors bibliothèque jamais attribué ni cible, worker par transfert attribué, refus du
  doublon direct et transfert, refus du remplacement avec workers par transfert, détenteur de secours en
  cas de panne d'un montage partagé, worker intégré en mode direct et par transfert.

## Retour arrière

Vider `library_transfer_workers` ramène au modèle actuel (accès direct seulement). Rétablir l'ouverture
« tout relais » dans `claim` n'est pas souhaitable : elle contredit le mode « aucun accès ».
