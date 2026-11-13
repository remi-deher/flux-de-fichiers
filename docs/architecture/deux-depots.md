# Architecture des dépôts applicatif et plugins

État au 9 novembre 2026. Cette note sépare le fonctionnement déjà livré, les
frontières préparées dans le socle et les capacités encore à concevoir. Elle ne
fige ni le format de manifeste, ni le mécanisme de distribution des plugins.

## Dépôt applicatif `flux-de-fichiers`

Ce dépôt conserve le binaire Rust unique et les migrations SQL. Il porte les
services d'exécution :

| Responsabilité | État / emplacement |
| --- | --- |
| `instance` | Choix du rôle au démarrage dans `src/instance.rs` ; central par défaut ou worker distant. |
| `central` | API Axum, accès PostgreSQL et lancement de l'exécuteur local facultatif ; `src/api.rs`, façade `src/coordinator.rs`. |
| `execution` | Remux MKVToolNix et validation des médias ; `src/media.rs`, façade `src/execution.rs`. |
| `workers` | Worker distant HTTPS et exécuteur intégré au central ; `src/worker.rs`, `src/local_worker.rs`, façade `src/workers.rs`. |
| `storage` | Sources, empreintes, fichiers de résultat et publication ; `src/storage.rs`, `src/api/transfers.rs`. |
| `security` | Autorisation API, TLS et configuration des secrets ; `src/auth.rs`, `src/config.rs`, façade `src/security.rs`. |
| `flows` | Emplacement réservé aux définitions de flux ; aucun moteur ni catalogue n'est livré. |
| `plugins` | Frontière réservée aux contrats consommés par l'application ; aucun chargeur n'est livré. |
| Interface | Client Leptos compilé en WASM (`client/`), page et thème générés par `frontend/`, servis par Axum sur le port 3000 (ADR 0014). |

Les façades nommées rendent les responsabilités visibles sans déplacer ni
remplacer la chaîne existante. PostgreSQL reste l'autorité sur les tâches et les
baux ; les migrations appartiennent à cette application. Les workers distants
passent par l'API authentifiée en TLS et ne se connectent pas à PostgreSQL.

## Dépôt `flux-de-fichiers-plugins`

Le dépôt séparé accueillera les ressources extensibles : plugins, outils,
traitements, flux prédéfinis et ressources complémentaires (par exemple polices,
images ou sons). Sa structure de distribution reste à valider conjointement.
Le dépôt applicatif ne dépend pas directement de son arborescence ni ne copie ses
fichiers au build à ce stade.

Chaque ressource devra porter un manifeste individuel lisible par l'application,
avec au minimum les informations validées : icône, description, auteur, version,
droits et licence. Un catalogue global devra permettre à l'application de
découvrir les ressources disponibles et leur emplacement. Le format, les champs
obligatoires, les versions de contrat, les signatures, l'installation, les mises
à jour et la compatibilité restent à concevoir ; aucune API Rust ou sérialisation
n'est figée dans ce squelette.

## Frontières et responsabilités futures

```text
flux-de-fichiers (application)
  API + PostgreSQL + baux + fichiers + sécurité
  exécution MKVToolNix locale ou par workers distants
  futur lecteur de catalogue/manifeste ─────────┐
                                                 │ contrat à définir
flux-de-fichiers-plugins (catalogue)             │
  ressources + manifeste individuel + index ────┘
```

Le dépôt d'outillage porte les déploiements et dépendances d'infrastructure,
notamment PostgreSQL ; il ne reçoit pas les migrations de l'application. La
décision utilisateur du 9 novembre 2026 formalise le modèle Bare Core et la
distribution d'outils par plugins (Option B, documentée dans l'ADR 0004 et dans
`docs/architecture/socle-execution-outils-distribues.md`). La haute disponibilité
et la bascule entre centraux ne sont pas impliquées par l'existence de plusieurs workers.
