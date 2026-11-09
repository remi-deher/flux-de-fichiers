# Contrôle #38 — démarrage flux-essai TRUENAS et frontend :18480

**Date** : 2026-11-09  
**Accord** : utilisateur explicite (via CoS)  
**NAS** : TRUENAS `192.168.1.50` (SSH `ssh-1794043060358-585bec`)

## Actions

1. Constat initial : `flux-essai` déjà `RUNNING` (preview `:18480`, API `:18443`).
2. Démarrage demandé exécuté :
   ```bash
   midclt call -j app.start '"flux-essai"'
   ```
   Résultat : `Started 'flux-essai' app` (100 %).  
   Note : `-job` n'est pas reconnu par midclt sur cette version ; `-j` est la forme correcte.
3. Re-query : état `RUNNING`, containers preview/coordinator/db `running`.

## Vérifications HTTPS

| URL | Résultat |
|-----|----------|
| `https://192.168.1.50:18480/` | **200** `text/html` |
| `https://192.168.1.50:18443/health` | **204** |
| `https://192.168.1.50:18480/health` | **204** (proxy) |
| `https://192.168.1.50:18443/api/v1/health` | 404 (chemin non exposé ; health = `/health`) |

## Worker ARTEMIS

- App `flux-essai-worker` : **RUNNING** (`test-a759c6179cdc`)
- Central ARTEMIS `flux-essai` : **STOPPED** (topologie préservée)
- API `GET /api/v1/workers` : worker `artemis-essai` présent, `last_seen` frais (&lt; 2 s)

## Preuve machine

Voir `verify-start-38.json`.

## URL canonique

Frontend : https://192.168.1.50:18480/  
API : https://192.168.1.50:18443/
