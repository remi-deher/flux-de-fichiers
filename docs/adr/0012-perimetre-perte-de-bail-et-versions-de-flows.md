# 0012 — Périmètre, perte de bail et versions de flows

Statut : accepté par l'utilisateur (13 nov. 2026).

## Contexte

L'analyse du projet de référence `encoding-central`
([audit du 13 nov. 2026](../audits/2026-11-13-analyse-encoding-central.md)) a relevé des écarts
avec la réécriture : périmètre média ambigu (README « sans réencodage », ADR 0006 tournée vers
l'encodage), remise en file automatique d'une tâche après expiration du bail, flows modifiables
sans version, absence d'IA et de MCP.

## Décisions

1. **Périmètre média : encodage par plugins.** Le remux seul servait aux essais. L'encodage
   (FFmpeg, encodeurs matériels) arrive par les plugins et les outils distribués (ADR 0004,
   0006, 0007), jamais embarqué dans l'image. Remplace la phrase « FFmpeg réservé aux médias
   synthétiques des tests » de `AGENTS.md` et « sans réencodage » du README.
2. **Perte de bail : échec, relance manuelle.** Un bail expiré fait échouer la tâche avec le
   code `lease_expired`, sans la remettre en file. Le mécanisme de relance est conservé pour
   être intégré plus tard : `POST /api/v1/jobs/{id}/retry` (administrateur) remet en file une
   tâche échouée pour `lease_expired`, dans la limite de `MAX_ATTEMPTS` (3). Aucune autre
   erreur n'est relançable.
3. **Flows versionnés.** Chaque enregistrement qui change le nom, la description, le masque
   ou la définition crée une version immuable (`flow_versions`) ; un contenu identique garde
   la version courante. Une tâche retient à sa création le flow par défaut et sa version
   (`jobs.flow_id`, `jobs.flow_version`) et s'exécute avec cette version. Modifier un flow ne
   relance ni ne modifie aucune tâche : il n'y a pas d'état à réconcilier. Supprimer un flow
   garde ses versions pour les tâches qui les citent. Seule la définition d'une version peut
   être réécrite, pour rechiffrer ses secrets lors d'une rotation de clé.
4. **IA et MCP dans le périmètre** de la réécriture ; conception à venir dans un ADR dédié.
5. **Installation neuve uniquement** : aucune reprise des données d'une installation
   `encoding-central` (`cluster.db`).
6. **Fenêtre d'installation ouverte** (ADR 0011) : choix assumé, pas de jeton de setup.

## Conséquences

- Migration `0012_flow_versions.sql` : table `flow_versions` (déclencheurs d'immuabilité),
  colonnes `flows.version`, `jobs.flow_id`, `jobs.flow_version`.
- Le worker intégré lit la version figée de la tâche (`repository::get_flow_version`), plus
  le flow par défaut courant.
- Événement SSE `job_retried` à la relance manuelle.
- Reste à faire : bouton de relance dans l'interface, encodage par plugins, IA et MCP.
  Le vocabulaire est unifié par l'[ADR 0013](0013-vocabulaire-central-worker.md).

## Retour arrière

Revenir sur ces points demande une nouvelle migration (les migrations ne se modifient pas) ;
la relance automatique se rétablit en refaisant passer en `queued` un bail expiré dans
`repository::claim`.
