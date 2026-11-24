# 0031 — Essai d'un script sur un worker

Statut : proposé ; sous-lots 3a et 3b écrits le 24 nov. 2026 (3b en attente de la CI), 3c à venir. Lot 3 de l'ADR 0030 ; lieu d'essai (un worker choisi) décidé par l'utilisateur le 24 nov. 2026.

## Contexte

L'éditeur de script (ADR 0030) enregistre un script sans jamais l'exécuter : l'erreur d'un interpréteur absent, d'un
JSON mal formé ou d'un `nextPort` inconnu n'apparaît qu'au premier flux réel. Le moteur (`execute_script_operation`,
ADR 0017 et 0025) exécute le script sur le worker de la tâche, avec le `PATH` et les outils de cette machine ; seul un
essai sur un worker donne le même résultat que la production.

## Décision

1. **Bouton « Tester » dans la barre de l'éditeur.** Il envoie le code **en cours d'édition** (pas la version
   enregistrée), l'interpréteur, un exemple de `config` et de `vars.*` (objets JSON saisis dans un panneau « Essai »)
   et le worker choisi. Droit requis : gérer les plugins, comme l'enregistrement.
2. **Route centrale** `POST /api/v1/local/scripts/trial` (`worker_id`, `interpreter`, `code`, `entrypoint`,
   `config`, `vars`). Le worker intégré exécute l'essai dans le processus central ; un worker distant en ligne le
   reçoit par le courtier de demandes existant (`/api/v1/worker/script-trials/pending` en attente longue, puis
   `/api/v1/worker/script-trials` pour le rapport), sur le modèle de l'exploration de dossiers. Un worker hors
   ligne, ou qui ne répond pas dans le délai, répond `502` (`central_unavailable`), comme l'exploration de
   dossiers. Attente maximale : délai du script + marge, plafonné.
3. **Exécution.** Dossier de travail jetable ; fichier d'exemple vide (`exemple.mkv`) comme `workingFile` et
   `sourceFile` ; même entrée JSON, même `env_clear` (`PATH`, `TMPDIR`), même interpréteur que
   `execute_script_operation` ; délai de 30 s au plus. Le dossier est supprimé ensuite, quel que soit le résultat.
   Un essai n'écrit ni job, ni historique, ni fichier de la collection.
4. **Résultat** : `stdin` envoyé, `stdout` et `stderr` (tronqués à 64 Kio, marqués s'ils le sont), code de sortie,
   durée, indicateur de délai dépassé, et la réponse interprétée (`status`, `message`, `nextPort`, `variables`…) ou
   la raison pour laquelle `stdout` n'est pas une réponse valide. Les jetons et identifiants d'URL sont masqués
   comme dans les journaux (`mask_log_secrets`).
5. **Aucun secret** : l'essai n'utilise ni les réglages de plugin ni les variables de type secret ; seules les
   valeurs saisies dans le panneau sont envoyées.
6. **Une structure de réponse partagée** : `ScriptOutput` et `ScriptOutcomeStatus` passent dans le crate partagé,
   utilisés par le moteur et par le client ; le courtier d'exploration devient générique pour servir aux deux
   usages sans duplication.

## Phasage

- **3a — exécuteur et route** : types partagés, exécuteur d'essai, route centrale, worker intégré (un autre worker
  répond `400` en attendant 3b).
- **3b — worker distant (écrit)** : `RequestBroker<T, R>` (`src/request_broker.rs`) remplace le courtier propre à
  l'exploration de dossiers, qui en devient un cas particulier (`RemoteBrowseBroker`) ; routes worker
  `/api/v1/worker/script-trials/pending` et `/api/v1/worker/script-trials` ; boucle `serve_relayed` du worker,
  commune à l'exploration et à l'essai. Le `PATH` d'un essai est celui du processus du worker, comme celui du moteur.
- **3c — interface** : panneau « Essai » (choix du worker, `config`, `vars`), bouton, affichage du résultat.

## Conséquences

- Tests : exécuteur (sortie, code de sortie, délai, troncature, dossier supprimé), courtier (livraison, mauvais
  worker, expiration), route (droits, worker hors ligne, interpréteur absent), analyse de la réponse.
- Un interpréteur absent du worker choisi s'affiche comme tel : c'est justement ce que l'essai doit révéler.
- Exécuter du code arbitraire sur un worker est déjà le rôle du moteur ; la route ne l'élargit que par le droit
  « gérer les plugins », identique à celui d'enregistrer le script.
