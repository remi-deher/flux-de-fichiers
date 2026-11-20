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
- NAT simulé : le détenteur et le traitant sont placés sur deux réseaux bridge distincts ;
  le central appartient aux deux et le port de transfert est publié sur la passerelle du
  réseau du traitant. Le téléchargement via cette adresse a rendu 33 554 432 octets et le
  SHA-256 attendu.
- Repli : `GET /api/v1/jobs/{id}/source` a déclenché l'envoi du détenteur via le courtier du
  central ; les octets et le SHA-256 correspondent. Avec une adresse directe annoncée mais
  injoignable, le worker traitant a journalisé le repli automatique puis atteint l'opération
  média, qui échoue car les outils ne sont pas installés. Pour isoler le transport, cette
  seconde tâche a été créée dans la base jetable et des marqueurs de disponibilité factices
  ont permis de franchir le contrôle préalable des outils ; aucune opération média réelle
  n'a été validée. Le passage à l'opération implique que le téléchargement et sa vérification
  par taille et SHA-256 ont réussi.
- Concurrence sur l'image publiée : cinq réponses HTTP 200 ont été obtenues alors que les
  quatre premières étaient encore ouvertes. Le permis était libéré au retour des en-têtes,
  avant la fin de la réponse. Le correctif le conserve avec le flux jusqu'à sa fermeture ; un
  test tient quatre corps ouverts, attend HTTP 503 pour le cinquième et vérifie la libération.
- Expiration : après les 600 secondes réelles de validité, le serveur direct a refusé le
  billet avec HTTP 403. Le bail a été renouvelé pendant l'attente et était encore actif au
  moment du refus.

## Limites

Le NAT Docker ne démontre pas le fonctionnement derrière un routeur physique entre machines.
L'image publiée précède le correctif ; la CI valide celui-ci,
mais une image incluant le correctif devra être essayée pour confirmer le plafond sur réseau.
