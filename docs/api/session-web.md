# Contrat de session web

Ces routes permettent au frontend same-origin d'utiliser les routes
d'administration sans conserver `ADMIN_KEY` dans le navigateur. L'API Bearer
existante reste disponible pour les outils et les agents ; les workers
continuent d'utiliser `WORKER_KEY`.

## Configuration

- `WEB_ORIGIN` : origine exacte autorisée, sans chemin, par exemple
  `http://localhost:3000` en développement ou `https://flux.example.net` en
  déploiement. Défaut : `http://localhost:3000`. Toute connexion, déconnexion
  ou écriture par cookie exige cette valeur dans l'en-tête `Origin`.
- `WEB_SESSION_TTL_SECONDS` : durée absolue d'une session, de 60 à 2 592 000
  secondes. Défaut : 28 800 secondes (8 heures). La session n'est pas prolongée
  à chaque requête.
- Le cookie `flux_session` porte `HttpOnly; SameSite=Strict; Path=/` et ajoute
  `Secure` lorsque `WEB_ORIGIN` utilise HTTPS. `flux_csrf` a les mêmes attributs
  sauf `HttpOnly`, afin de permettre au frontend de le lire. Les deux expirent
  avec la session. Les réponses de session utilisent `Cache-Control: no-store`.

## Cycle frontend

1. `POST /api/v1/auth/session` avec `Content-Type: application/json`, l'en-tête
   `Origin: <WEB_ORIGIN>` et le corps `{"admin_key":"…"}`. Une clé valide pose
   les cookies et renvoie `{csrf_token, expires_at}`. La clé n'est jamais
   renvoyée ni enregistrée dans la session.
2. Le navigateur envoie automatiquement les cookies same-origin. Les lectures
   (`GET`) des routes d'administration acceptent la session cookie.
3. Pour `POST /api/v1/jobs` et `POST /api/v1/auth/logout`, envoyer aussi
   `Origin: <WEB_ORIGIN>` et `X-CSRF-Token` égal au cookie `flux_csrf`. Le serveur
   vérifie le double-submit et le hachage CSRF associé à la session en base.
4. Après rechargement, `GET /api/v1/auth/session` valide les cookies et renvoie
   à nouveau le jeton CSRF et l'expiration. Cette lecture n'exige pas l'en-tête
   CSRF et ne modifie pas l'état.
5. `POST /api/v1/auth/logout` vérifie Origin et CSRF, supprime le hachage de
   session en PostgreSQL, puis expire les deux cookies. L'expiration est aussi
   vérifiée côté serveur.

Les origines absentes, `null` ou différentes sont refusées. Les routes admin
acceptent toujours `Authorization: Bearer <ADMIN_KEY>`. Si cet en-tête est
présent, il a priorité sur la session : une valeur invalide renvoie 401 sans
repli sur les cookies. Supprimer l'en-tête pour utiliser la session navigateur.
Une requête Bearer ne repose pas sur les cookies et n'est pas soumise au contrôle CSRF. Les endpoints
worker conservent `WORKER_KEY` et leur protocole actuel.

## Téléchargements en flux

`GET /api/v1/jobs/{id}/output` reste protégé par Bearer ou session valide. Il
renvoie le fichier avec `Content-Length`, `Content-Disposition: attachment`,
`Cache-Control: no-store` et
un corps streamé depuis le fichier, sans tampon média en mémoire serveur. Avec
une session same-origin, le frontend doit déclencher le téléchargement par un
lien/navigateur vers l'URL (attribut `download` possible), plutôt que lire le
corps avec `Response.blob()`. Le proxy Compose garde `proxy_buffering off` pour
`/api/`, afin de transmettre le corps sans tampon en mémoire.

`frontend/` n'est pas modifié dans cette tâche ; son interface actuelle utilise
encore le Bearer et `Blob`. Ce document et `docs/openapi.yaml` décrivent le
contrat cible d'intégration. En production, régler `WEB_ORIGIN` sur l'origine
HTTPS réellement utilisée par le navigateur.
