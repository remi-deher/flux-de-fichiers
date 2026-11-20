# Essai du partage des sources — 20 novembre 2026

## Dispositif

Image publiée `mrcryllix/forgearr:0.61.0` (digest
`sha256:642cdb1b42275c33d8e8eca53520a0944d5e8deed0fbea539a4077a4781ad85e`),
central et deux workers dans des conteneurs Docker sur un réseau bridge. Le worker direct
annonce `WORKER_TRANSFER_URL` et écoute sur `WORKER_TRANSFER_PORT` ; son port est également
publié sur l'hôte. La bibliothèque associe le détenteur direct au worker par transfert.
Le fichier d'essai fait 33 554 432 octets ; il est scanné par le détenteur.

Pour observer séparément les voies, un bail a été attribué au worker par transfert après
réinitialisation de la tâche dans la base **jetable du laboratoire**. Les requêtes ont été
émises depuis le conteneur traitant avec ce bail et son billet, sans lancer son traitement
média. Aucun secret du laboratoire n'est conservé dans ce compte rendu.

## Résultats

- Voie directe : HTTP 200 ; taille et SHA-256 correspondent à la source. Une requête via le
  port publié sur l'hôte a également abouti.
- Repli : `GET /api/v1/jobs/{id}/source` a déclenché l'envoi du détenteur via le courtier du
  central ; les octets et le SHA-256 correspondent. Une adresse directe volontairement
  injoignable a été vérifiée séparément. Le basculement automatique du worker après cet
  échec n'a pas été observé dans cet essai manuel.
- Concurrence sur l'image publiée : cinq réponses HTTP 200 ont été obtenues alors que les
  quatre premières étaient encore ouvertes. Le permis était libéré au retour des en-têtes,
  avant la fin de la réponse. Le correctif le conserve avec le flux jusqu'à sa fermeture ; un
  test tient quatre corps ouverts, attend HTTP 503 pour le cinquième et vérifie la libération.

## Limites

Le port publié confirme un accès à travers le réseau Docker local ; il ne démontre pas le
fonctionnement derrière un NAT physique entre machines. Un essai de bout en bout avec une
adresse directe annoncée mais injoignable reste nécessaire pour valider le basculement
**automatique** du worker. L'image publiée précède le correctif ; la CI valide celui-ci,
mais une image incluant le correctif devra être essayée pour confirmer le plafond sur réseau.
