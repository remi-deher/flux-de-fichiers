# ADR 0015 — Mot de passe oublié par courriel (SMTP)

Statut : accepté (complète l'ADR 0011, qui prévoyait seulement la réinitialisation en ligne de commande).

## Contexte

La réinitialisation passait uniquement par `flux-de-fichiers reset-password` dans le conteneur.
Les comptes n'avaient pas d'adresse e-mail. Un écran « mot de passe oublié » exige donc une adresse
par compte et un moyen d'envoyer un message.

## Décisions

1. **Adresse e-mail facultative par compte** (`accounts.email`, unique sans tenir compte de la casse).
   Elle se renseigne à l'installation du central, à la création d'un compte ou par un administrateur
   (`PUT /api/v1/accounts/{id}`, chaîne vide pour l'effacer). Un compte sans adresse ne peut pas
   utiliser le courriel ; la ligne de commande reste le recours.
2. **SMTP par variables d'environnement**, jamais en base : `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS`
   (`starttls` par défaut, `tls`, `none`), `SMTP_USERNAME`, `SMTP_FROM`, et `SMTP_PASSWORD` ou
   `SMTP_PASSWORD_FILE` (secret fourni par fichier). Sans `SMTP_HOST` ou sans `WEB_ORIGIN` (qui
   sert à bâtir le lien), la fonction est désactivée et l'écran ne propose pas le lien.
3. **Jeton** : 32 octets aléatoires, envoyé dans un lien `<WEB_ORIGIN>/#reset=<jeton>` (fragment :
   il ne part ni dans les journaux du serveur ni dans l'en-tête `Referer`). Seul le haché SHA-256 est
   stocké (`password_reset_tokens`). Il vit 30 minutes, sert une seule fois, et une nouvelle demande
   remplace les précédentes.
4. **Routes publiques** : `POST /api/v1/auth/forgot-password` répond `202` de la même façon que le
   compte existe, soit désactivé ou inconnu, et même si l'envoi échoue. Limites : 5 demandes par
   quart d'heure et par adresse saisie, et un envoi au plus par minute et par compte.
   `POST /api/v1/auth/reset-password` consomme le jeton, change le mot de passe et ferme toutes les
   sessions du compte. Tout est journalisé localement, sans adresse ni jeton en clair.
5. **Worker** : le changement de mot de passe reste au central (les comptes d'un worker en sont une
   copie). L'écran d'un worker ne propose pas le lien.

## Conséquences

- Nouvelle dépendance `lettre` (SMTP, rustls), non embarquée dans une image à part.
- Le transport SMTP est derrière un trait (`MailSender`) : les tests n'ont besoin d'aucun serveur.
- Limite : sans TLS (`SMTP_TLS=none`), le message, donc le lien, circule en clair jusqu'au relais.
