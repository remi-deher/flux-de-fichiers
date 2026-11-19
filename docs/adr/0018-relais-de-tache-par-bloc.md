# 0018 — Relais de tâche entre workers par bloc de flux

Statut : proposé (19 nov. 2026), orientation validée par l'utilisateur (relais par blocs dédiés,
sondes matérielles légères dans l'image, relais avant tout traitement). Cet ADR pose les bases ;
le relais en cours de flux viendra ensuite.

## Contexte

Un worker peut ne pas avoir la capacité d'exécuter une tâche, par exemple encoder en H.265 alors
que son GPU ne le permet pas. Il doit pouvoir la laisser à un autre worker qui en est capable.

État actuel :

- L'attribution est **pull** : `repository::jobs::claim` ne filtre que sur `target_worker_id` et sur
  les contraintes de source (`source_worker_id`, `verified_job_source_mounts`). Aucune capacité
  n'est déclarée ni comparée ; la télémétrie ne remonte que `gpu_devices`.
- Un bail expiré fait échouer la tâche (`lease_expired`), sans remise en file silencieuse
  ([ADR 0012](0012-perimetre-perte-de-bail-et-versions-de-flows.md), point 2).
- Toute exécution passe par les blocs d'un flux
  ([ADR 0009](0009-plugins-comme-blocs-d-action-et-inspection-implicite.md)), avec aiguillage
  conditionnel ([ADR 0008](0008-execution-in-place-et-aiguillage-conditionnel.md)) et contrats
  d'opérations ([ADR 0017](0017-moteur-generique-et-contrats.md)).
- Les fichiers circulent soit par montage partagé vérifié (`verified_job_source_mounts`), soit
  par le central avec bail (`src/api/transfers.rs`, empreinte `source_sha256`).

Référence : `encoding-central` (ADR 0016, bloc `nodes-handoff`) gèle la tâche, transfère les
artefacts avec SHA-256 puis active la reprise côté cible. Son explorateur distant est en lecture
seule ; aucun des deux projets n'offre d'écriture de fichiers à distance, et ce besoin n'en
requiert pas.

## Décision

1. **Le relais est un bloc de flux, jamais un mécanisme caché du `claim`.** Le flux décide, par
   aiguillage conditionnel, quand relayer (ex. « codec cible H.265 et capacité absente »).
   Conforme à l'ADR 0009.
2. **Capacités déclarées par les workers.** Chaque worker déclare ses capacités d'encodage
   (codec, accélération) à l'enregistrement et à chaque heartbeat. Types fermés dans
   `forgearr-shared` (enums Serde, pas de chaînes magiques). Elles s'affichent dans la fiche
   worker et servent de condition d'aiguillage.
   **Détection (décision utilisateur du 19 nov. 2026) :** l'image embarque des **sondes
   matérielles très légères**, pas des encodeurs : inventaire PCI (`lspci`), état du GPU NVIDIA
   (`nvidia-smi`), équivalents Intel et AMD (ex. `vainfo`). Le worker les interroge au démarrage
   et au heartbeat pour dériver ses capacités. Cela précise sans la contredire l'ADR 0012,
   point 1 : FFmpeg et les encodeurs restent fournis par plugins, jamais dans l'image.
   Réserve : `nvidia-smi` est généralement injecté par l'hôte (NVIDIA Container Toolkit) et non
   installé dans l'image ; son absence signifie « pas de GPU NVIDIA exposé », pas une erreur.
   Une sonde qui échoue ou est absente ne fait déclarer aucune capacité correspondante.
3. **Bloc natif « Relayer la tâche »** (runtime `primitive`, car il agit sur le bail et la file,
   ce qu'un plugin ne peut pas faire). Paramètres : cible = worker précis, étiquette ou capacité
   requise ; mode strict optionnel.
4. **Effet du relais :**
   - le worker enregistre le point de reprise : nœud suivant, fichier de travail courant,
     métadonnées du média, flux figé (`flow_id`, `flow_version`) ;
   - il libère volontairement son bail (époque incrémentée) ; le job repasse `queued` avec une
     contrainte de cible ;
   - `claim` n'attribue le job qu'à un worker qui satisfait la cible ;
   - le repreneur reprend au nœud suivant, avec la version figée du flux.
5. **Exception encadrée à l'ADR 0012.** La règle « pas de remise en file » vise la **perte
   involontaire** du bail ; elle reste inchangée. Le relais est **volontaire, journalisé
   (événement dédié), émis par un bloc explicite, avant tout effet non rejouable** et borné par un
   nombre maximal de relais par tâche (constante nommée). Un bail expiré après un relais reste un
   échec `lease_expired`.
6. **Pas de cible compatible = échec visible.** Si aucun worker enrôlé ne satisfait la cible, la
   tâche échoue avec un code dédié (`no_capable_worker`) ; en mode non strict, elle peut attendre
   un délai borné avant d'échouer. Jamais d'attente infinie silencieuse.
7. **Accès au fichier par le repreneur** (avant tout traitement : seule la source est concernée) :
   - montage partagé commun : `verified_job_source_mounts`, sans transfert ;
   - sinon : le repreneur récupère la source et le fichier de travail via le central
     (`transfers.rs`), avec vérification SHA-256 et reprise sur bail ; la sortie revient par le
     même canal. Une empreinte invalide interrompt le relais sans activer la cible.
8. **Hors périmètre de cet ADR** (non demandé) : écriture ou suppression de fichiers à distance,
   file de requêtes d'exploration persistée en SQLite, verrous de disque, haute disponibilité.
   Chacun demandera son propre ADR.

## Décisions de cadrage (réponses de l'utilisateur, 19 nov. 2026)

- **Détection des capacités** : sondes matérielles légères embarquées dans l'image (point 2).
  Le matériel présent n'est pas une preuve d'encodage fonctionnel : une confirmation par un
  test d'encodage réel, avec l'outil FFmpeg du cache de plugins, pourra être ajoutée ensuite.
- **Moment du relais** : **avant tout traitement uniquement** dans cette première version. Le
  relais en cours de flux, avec transfert du fichier de travail, est reporté (« on le fera
  ensuite »). Le bloc refuse donc de relayer s'il existe déjà un fichier de travail.

## Conséquences

- Migration à venir (`0031_*`) : capacités des workers, contrainte de cible et compteur de relais
  du job, trace du point de reprise. Les migrations existantes ne changent pas.
- `Dockerfile` : ajout des sondes légères (construction des images sur GitHub CI uniquement).
- Nouveau code d'erreur de job `no_capable_worker` et événement de journal `job_relayed`.
- Le moteur de flux gagne une sortie de type « relais » qui suspend l'exécution locale sans erreur.
- L'éditeur de flux expose le bloc et la condition « capacité du worker courant ».
- Tests natifs : relais vers un worker compatible, aucune cible, plafond de relais, empreinte
  corrompue, reprise au bon nœud (voir plan de vérification dans le plan de mise en œuvre).

## Retour arrière

Retirer le bloc du catalogue et ignorer les capacités dans `claim` rétablit le comportement
actuel ; les colonnes ajoutées restent inertes (une migration de retrait serait nécessaire pour
les supprimer).
