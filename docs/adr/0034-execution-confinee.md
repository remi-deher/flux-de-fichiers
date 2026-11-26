# 0034 — Exécution confinée des processus externes

Statut : proposé le 26 nov. 2026 ; phase 0 faite et phase 1 écrite le 26 nov. 2026 (CI verte). Demande de l'utilisateur : pouvoir exécuter n'importe quel binaire ou langage fourni par le dépôt de plugins, tout en gardant une forme de sandboxing, en Rust seul et simple à mettre en place. Complète les ADR 0004 (confinement strict de l'exécution), 0012, 0017, 0025 et 0031 ; s'applique d'abord aux serveurs de langage de l'ADR 0032.

## Contexte

Le central et les workers lancent des binaires externes à sept endroits : les outils de plugin (`src/tool_run.rs`), les scripts du moteur (`src/flows/script_runner.rs`), l'essai de script (`src/script_trial.rs`), les sondes (`src/software_probe.rs`, `src/hardware_probe.rs`, `src/flows/binary_runner.rs`) et, bientôt, les serveurs de langage (ADR 0032). Le confinement actuel tient en trois choses :

- le conteneur : utilisateur non-root, système de fichiers en lecture seule, `cap_drop: ALL`, `no-new-privileges` (`Dockerfile`, `compose.yaml`) ;
- le processus : `env_clear`, `PATH` fixé, groupe de processus tué à l'abandon (`SupervisedChild`, ADR 0027) ;
- les binaires : empreinte SHA-256 vérifiée avant exécution (ADR 0004).

Rien ne confine un processus par rapport à un autre ni par rapport au reste du conteneur : un script peut lire tout le système de fichiers visible (dont `/config/flux.db`, les clés auto-amorcées et les bibliothèques), ouvrir le réseau et épuiser la mémoire. L'ADR 0004 promet un « confinement strict » que seul le conteneur tient. Le risque grandit avec la liberté voulue : n'importe quel langage, n'importe quel binaire.

## Décision (proposée)

1. **Un seul point d'entrée.** Tout lancement de processus externe passe par une fonction unique du module `confine` (`run_confined`, qui remplace les appels directs à `Command::new` listés plus haut). Aucun autre code ne lance de binaire externe ; un test de structure le vérifie (recherche de `Command::new` hors du module).
2. **Un profil typé.** `ConfinementProfile` (énumérations Serde, dans `shared/` pour qu'un manifeste ou l'interface puisse le décrire) :
   - dossiers en lecture seule et dossiers inscriptibles (liste de chemins) ;
   - réseau : `none` ou `any` ; aucune adresse ni liste de ports au départ ;
   - variables d'environnement admises, en plus de `PATH` et `TMPDIR` ;
   - plafonds : mémoire, nombre de processus, taille des fichiers écrits, temps processeur, durée totale ;
   - exécutables lançables : l'exécutable demandé et son dossier d'outil, jamais le reste du système.
   Le profil par défaut interdit tout : système de fichiers limité au dossier d'outil (lecture) et au dossier de travail jetable (écriture), pas de réseau, plafonds bas. Tout élargissement est écrit dans le profil.
3. **Profils par usage.**
   - serveur de langage : lecture du dossier d'outil, écriture dans un dossier jetable, pas de réseau ;
   - script du moteur et essai : lecture du dossier de travail du job (et des fichiers source déclarés), écriture du dossier de travail seulement, réseau `none` sauf demande du script, qui l'affiche dans l'éditeur et les informations du script ;
   - outil de plugin : profil du manifeste (ADR 0004, `garanties-execution-v1`), par défaut lecture du média source, écriture de la sortie, pas de réseau ;
   - sondes : lecture seule de ce qu'elles inspectent.
   Le réseau d'un script est un choix explicite de l'auteur, jamais implicite.
4. **Mécanisme noyau, sans outil externe ni droit root.** Le processus s'applique les restrictions à lui-même juste avant de lancer le binaire :
   - **Landlock** : accès aux fichiers et, sur noyau 6.7 ou plus, connexions et écoutes TCP ;
   - **seccomp** : liste d'appels système refusés (montage, `ptrace`, `bpf`, `keyctl`, chargement de modules, changement d'espace de noms, etc.), `EPERM` en retour, processus tué si l'architecture d'appels change ;
   - **rlimits** (`setrlimit`) : données du processus (`RLIMIT_DATA`, pas l'espace d'adressage que V8 réserve sans l'utiliser), taille des fichiers, temps processeur, descripteurs ouverts, aucun fichier de vidage mémoire ;
   - `PR_SET_NO_NEW_PRIVS`, nouvelle session et groupe de processus (déjà fournis par `SupervisedChild`).
   Aucun de ces mécanismes ne demande de capacité : `cap_drop: ALL` reste valable. Les appels système sont faits directement par `libc` (déjà une dépendance) : pas de crate de plus, donc pas de changement de `Cargo.lock`. Le nombre de processus n'a pas de plafond par `RLIMIT_NPROC` : il compte tous les processus de l'utilisateur, central compris ; le conteneur le borne déjà (`pids_limit`).
5. **Assistant dans le même binaire.** Les restrictions s'appliquent par une sous-commande de l'exécutable Forgearr (`forgearr sandbox-exec --profile <json> -- <commande> <arguments>`) qui s'applique le profil puis remplace son processus par la commande (`exec`). Cela évite d'écrire du code `unsafe` dans `pre_exec` d'une tâche asynchrone, garde un seul exécutable dans l'image (ADR 0005) et se teste sans lancer de vrai script. Le profil passe en argument ou par un descripteur, jamais dans l'environnement du processus confiné.
6. **Détection et politique par worker.** Au démarrage, chaque worker détecte la version de Landlock et la disponibilité de seccomp, les publie avec sa télémétrie (ADR 0004) et les affiche sur sa fiche. Un réglage central choisit, par défaut `exiger` : un job dont le profil ne peut pas être appliqué échoue avec une raison claire ; `avertir` exécute sans le mécanisme manquant et le journalise, jamais en silence. Les noyaux anciens (NAS ARM) se voient ainsi, au lieu de se découvrir à l'exécution.
7. **Compatibilité du conteneur à vérifier avant le code.** Un essai sur la CI doit confirmer que le profil seccomp par défaut de Docker laisse passer les appels Landlock, sur les noyaux des machines cibles (x86_64 et aarch64). Si ce n'est pas le cas, le profil du conteneur (`security_opt`) est précisé dans `compose.yaml` ; sans solution acceptable, l'ADR est révisée avant tout code.

## Ce que le confinement ne garantit pas

- Ce n'est pas une isolation de machine virtuelle : le noyau est partagé avec l'hôte, une faille du noyau reste exploitable.
- Le réseau n'est pas filtré par adresse : `none` ou `any` (Landlock ne filtre ni UDP ni les adresses). Un filtrage plus fin demanderait des espaces de noms réseau, écartés ici car ils exigent des droits que le conteneur n'a pas.
- Aucun plafond de mémoire collectif par cgroup ; les rlimits bornent chaque processus. Un job qui lance de nombreux processus reste borné par le plafond de processus.
- Un plugin qui a besoin du réseau (Sonarr, webhook, ADR 0009) le déclare ; il n'est pas confiné contre lui-même.

## Résultat de la phase 0 (CI, 26 nov. 2026)

Sonde de `scripts/confinement-probe/` sur les machines de GitHub (noyau 7.0, Landlock version 8), dans un conteneur
réglé comme `compose.yaml` (non-root, lecture seule, `cap_drop: ALL`, `no-new-privileges`) :

- **Landlock et seccomp passent avec le profil seccomp par défaut de Docker**, sur x86_64 comme sur aarch64 : lecture et
  écriture refusées hors des dossiers autorisés, connexion TCP refusée, filtre seccomp appliqué. `compose.yaml` n'a pas
  à changer (point 7 levé pour ces noyaux).
- Non vérifié : les noyaux des NAS (Landlock absent avant 5.13, réseau avant 6.7) : la détection du point 6 sert à cela.
- `ty` 0.0.111 : complétion, infobulle et temps de réponse conviennent (150 à 160 suggestions pour `sys.`, quelques
  millisecondes, 48 à 50 Mio de mémoire). À corriger côté client : les attributs spéciaux (`__class__`…) viennent en
  tête d'une valeur sans type connu, et `ty` lit les paquets du Python de la machine (`requests` y était résolu).
  Diagnostics : trois passes (poussés, à la demande, configuration répondue, script écrit sur disque) n'ont rendu
  qu'une liste vide pour un script contenant un nom inconnu. Cause non trouvée ; à élucider au début du lot b0 de
  l'ADR 0032 (essai avec un `pyproject.toml`, un `ty.toml` ou un répertoire hors de `/tmp`), sinon repli sur `pyright`.

## Phasage

- **0 — essai de compatibilité (faite)** : Landlock et seccomp dans le conteneur par défaut, sur x86_64 et aarch64, sur la CI.
- **1 — module `confine` et sous-commande `sandbox-exec` (écrite)** : profil typé (`shared/src/confinement.rs`), application des restrictions, détection, tests unitaires et `tests/confinement.rs` ; aucun appelant migré.
- **2 — serveurs de langage** (lot b1 de l'ADR 0032) : premier usage réel, sans migration d'un comportement existant.
- **3 — scripts du moteur et essai** : profil par défaut, déclaration du réseau dans le script et l'éditeur, réglage `exiger` ou `avertir`.
- **4 — outils de plugin et sondes** : profil dans le manifeste (schéma du dépôt de plugins à étendre), migration des appels directs, test de structure.

## Conséquences

- Tests : application du profil (lecture et écriture refusées hors dossiers déclarés, réseau refusé, plafond de mémoire et de processus, durée), détection de capacité, échec explicite quand le mécanisme manque, absence d'appel direct à `Command::new`.
- Un script qui lisait des fichiers hors de son dossier de travail ou ouvrait le réseau sans le déclarer échouera après la phase 3 : à annoncer dans les notes de version et à afficher dans l'éditeur (ADR 0030).
- L'ADR 0004 (confinement strict) devient vrai du point de vue de chaque processus, pas seulement du conteneur.
- La pile ne change pas : Rust, binaire unique, aucun outil de sandbox ajouté à l'image.
