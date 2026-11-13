# 0013 — Vocabulaire : central et worker

Statut : accepté par l'utilisateur (13 nov. 2026).

## Contexte

Le code, l'interface et la documentation mêlaient coordinateur, centrale, agent, nœud et worker
pour deux rôles seulement. Le projet de référence `encoding-central` a dû y consacrer une campagne
entière (son ADR 0040) ; l'installation neuve (ADR 0012) permet de corriger sans compatibilité.

## Décision

- **Central** (nom masculin) : l'installation qui porte l'API, l'interface, la base et l'autorité
  sur les tâches. Il embarque son worker intégré.
- **Worker** : toute installation qui traite des tâches ; il est rattaché à un seul central.
- **Installation** : terme neutre pour une instance dont le rôle n'est pas encore choisi, ou pour
  parler des deux rôles à la fois.
- « Nœud », « agent », « coordinateur » et « centrale » ne désignent plus une machine. « Nœud »
  reste réservé aux blocs d'un graphe de flux (`nodes` d'une définition), et « agent » aux agents
  d'IA.

## Conséquences (contrat cassant, sans reprise)

| Avant | Après |
|---|---|
| argument `coordinator`, service compose `coordinator` | `central` |
| `COORDINATOR_URL` | `CENTRAL_URL` |
| `/api/v1/nodes/enroll` | `/api/v1/workers/enroll` |
| `/api/v1/node/status` | `/api/v1/instance/status` |
| `/api/v1/node/{overview,jobs,plugins,login,accounts}` | `/api/v1/worker/…` |
| champs `coordinator_url`, `node_name`, `nodeKind`, `coordinatorUrl`, `coordinatorReachable` | `central_url`, `worker_name`, `instanceKind`, `centralUrl`, `centralReachable` |
| droits `view_nodes`, `manage_nodes`, `node_agent` | `view_workers`, `manage_workers`, `worker_protocol` |
| bibliothèque : champ `nodes`, table `library_nodes` | `workers`, `library_workers` (migration 0013) |
| fichier `/config/node.json` | `/config/worker.json` |
| worker intégré `coordinator-local` | `central-local` |
| modules `node`, `node_settings`, `node_view`, `api::nodes` | `instance`, `worker_settings`, `worker_view`, `api::installation` |

Les ADR 0001 à 0011, les comptes rendus d'essais et les audits datés gardent leur vocabulaire
d'origine. Les documents de conception de `docs/architecture/` antérieurs (télémétrie,
ordonnancement, socle) ne sont pas réécrits.

## Retour arrière

Revenir en arrière demande une nouvelle migration (renommer `library_workers`) et un
renommage inverse ; aucune donnée n'est à convertir.
