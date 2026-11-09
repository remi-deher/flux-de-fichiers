# Déploiement frontend TRUENAS (#37) — 9 novembre 2026

> Preuve historique du **premier** déploiement (preview sur `:18444`).
> État courant du service : voir `addendum-port-18480-2026-11-09.md`.

## Cible (historique)

- **Frontend** : `https://192.168.1.50:18444/` (`WEB_ORIGIN` identique à l’époque)
- **API** : `https://192.168.1.50:18443/`
- **Image** : `mrcryllix/flux-de-fichiers:test-a759c6179cdc`
  - digest constaté : `sha256:d07cdf8ff76005cda6e483794cee7d6ccb81378890ef2894a5a5df6f95241f4b`
  - label OCI `revision` : `29d72847e7f8bbaa279869d4c178d3e5160c3d04` (contenu PR #9 / sessions)
  - version label : `0.2.0`
- **Nginx preview** : `nginx:1.28.0-alpine` + `frontend/dist/index.html` monté
- Snapshot avant mutation : `DATA/flux-de-fichiers-test@pre-frontend-37`

## Topologie conservée

| App | Machine | État |
|-----|---------|------|
| `flux-essai` | TRUENAS | RUNNING |
| `flux-essai-worker` | ARTEMIS | RUNNING (même image) |
| `flux-essai` | ARTEMIS | STOPPED |

Secrets, volumes Postgres/data/logs/input inchangés. Mutations via `midclt call -j app.update` uniquement.

## Preuves (historiques, exécutées contre `:18444`)

Rapport JSON : `verify-frontend-37.json` (14/14 OK), exécuté **sur TRUENAS** alors que le preview écoutait `:18444` :

- page frontend 200, proxy `/health` 204
- session cookie (`POST/GET /api/v1/auth/session`), liste jobs, logout
- remux `essai25-a.mkv` réussi + téléchargement SHA-256 (Bearer et cookie)
- remux `essai25-b.mkv` ciblé sur `artemis-essai` réussi

Ces 14 scénarios **ne** sont **pas** à relire comme s’ils avaient été joués sur `:18480`.

## Retour arrière

1. Restaurer le compose précédent (`test-794b64cf7d3b`, sans service `preview`) via `midclt call -j app.update flux-essai …`
2. Worker : même image précédente via `app.update flux-essai-worker`
3. Option dataset : `zfs rollback DATA/flux-de-fichiers-test@pre-frontend-37` (sudo) puis redéploiement — **détruit les écritures post-snapshot**

## Limites

- Pas de comparaison de flux FFmpeg (binaire absent sur TRUENAS)
- PR #10 (Playwright CI) encore en brouillon ; non déployée
- Certificats d’essai auto-signés (~30 j)
- `WEB_ORIGIN` HTTP LAN refusé par le binaire : frontend servi en HTTPS avec les certs d’essai
- Anciens enregistrements `coordinator-*` encore listés dans `/api/v1/workers`
