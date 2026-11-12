# Conception technique du cache worker et validation SHA-256 des outils distribués

Date : 9 novembre 2026. Statut : spécification d'architecture technique (ADR 0004).

## 1. Objectifs et principes de sécurité

Dans le modèle **Bare Core (Option B)**, aucun binaire tiers (tel que `mkvmerge` ou `ffmpeg`) n'est présent dans l'image de base du conteneur worker. Le worker a la responsabilité de maintenir un cache local sécurisé de binaires autonomes pour exécuter les tâches assignées.

Principes directeurs non négociables :
1. **Intégrité cryptographique stricte** : aucune exécution n'est autorisée sans vérification préalable et systématique de l'empreinte SHA-256 de l'artefact.
2. **Confinement de privilèges** :
   - Exécution sous l'utilisateur non-root standard du conteneur (`UID 10001`, `USER app`).
   - Interdiction formelle de montage du socket Docker de l'hôte (`/var/run/docker.sock`).
   - Interdiction de toute commande d'élévation de privilèges (`sudo`, `setuid`, `capabilities` super-utilisateur).
3. **Extraction sécurisée sans shell (`noShellUnpack: true`)** : le décompactage des archives est opéré directement par des bibliothèques Rust en mémoire (`flate2`, `tar`, `xz2`), sans jamais invoquer `/bin/sh -c`, `bash` ou des commandes utilitaires externes.
4. **Protection contre la traversée de répertoires** : contrôle strict contre les attaques *Zip Slip* : tout chemin d'archive contenant des segments `..`, des chemins absolus `/` ou des liens symboliques pointant hors du répertoire de destination provoque le rejet immédiat de l'archive.
5. **Support des environnements déconnectés** : fonctionnement configurable entre le téléchargement à la demande (`explicit-allowlist`) et le mode pré-amorcé hors ligne (`offline-preseeded`).

---

## 2. Structure et arborescence du cache local

Le cache réside sous une racine configurable (variable d'environnement `TOOLS_CACHE_DIR`, par défaut `/work/tools-cache` ou un volume persistant dédié).

### 2.1 Hiérarchie standardisée

```text
/work/tools-cache/
├── app.mkvtoolnix/
│   └── 88.0.0/
│       └── x86_64/
│           ├── .manifest.json        # Copie locale du contrat de distribution
│           ├── .lock                 # Verrou d'écriture exclusif (pendant acquisition)
│           ├── .ready                # Marqueur de présence et validation intègre
│           ├── .sha256               # Somme SHA-256 du binaire vérifié
│           └── bin/
│               └── mkvmerge          # Exécutable binaire final (mode 0755)
├── app.ffmpeg/
│   └── 7.1.0/
│       └── x86_64/
│           ├── .manifest.json
│           ├── .ready
│           ├── .sha256
│           └── bin/
│               └── ffmpeg            # Exécutable binaire final (mode 0755)
└── staging/                          # Zone de transit temporaire pour téléchargements
    └── .tmp_dl_9a8f...               # Archive en cours de téléchargement
```

### 2.2 Permissions du système de fichiers
- Répertoire parent et sous-dossiers : permissions Unix `0750` ou `0755`, possédés par `app:app` (UID 10001 / GID 10001).
- Binaire final extrait : permissions Unix `0755` (`-rwxr-xr-x`). Le bit exécutable est obligatoire (`executableBit: true`).
- Fichiers d'état et manifestes : permissions `0640` (`-rw-r-----`).

---

## 3. Cycle de vie d'acquisition et d'exécution d'un outil

Le processus d'accès à un outil suit une machine à états stricte :

```
       [ Demande d'outil (tool_id, version, arch) ]
                            │
                            ▼
           ┌────────────────────────────────┐
           │ Le binaire existe dans le      │
           │ cache avec le marqueur .ready ?│
           └──────┬──────────────────┬──────┘
             OUI  │                  │  NON
                  │                  ▼
                  │     ┌───────────────────────────────┐
                  │     │ Acquérir le verrou .lock      │
                  │     │ Télécharger l'archive vers    │
                  │     │ la zone staging               │
                  │     └──────────────┬────────────────┘
                  │                    │
                  │                    ▼
                  │     ┌───────────────────────────────┐
                  │     │ Contrôle SHA-256 de l'archive │
                  │     └──────┬──────────────────┬─────┘
                  │       OK   │                  │ ÉCHEC
                  │            ▼                  ▼
                  │     ┌──────────────┐   ┌──────────────┐
                  │     │ Extraction   │   │ Erreur :     │
                  │     │ native Rust  │   │ Checksum     │
                  │     │ sans shell   │   │ Mismatch     │
                  │     └──────┬───────┘   └──────────────┘
                  │            │
                  ▼            ▼
     ┌────────────────────────────────────┐
     │ Recalcul SHA-256 du binaire extrait│
     │ et comparaison avec binarySha256   │
     └─────────────────┬──────────────────┘
                 OK    │     ÉCHEC
          ┌────────────┴────────────┐
          ▼                         ▼
   [ Outil PRÊT ]            [ Erreur de Sécurité ]
   (Exécution directe)       (Purge immédiate du cache)
```

### Étape 1 : Contrôle du cache chaud
Avant toute exécution, le worker vérifie l'existence de l'exécutable et du marqueur `.ready` :
1. Calcul du hash SHA-256 du binaire présent sur disque (`bin/<binaryRelativePath>`).
2. Comparaison avec `binarySha256` renseigné dans le contrat ou `.sha256`.
3. Si le hash concorde : le binaire est déclaré immédiatement prêt (`cache_hit`). Aucun accès réseau ni décompression n'est requis.
4. Si le hash ne concorde pas : le cache local est considéré corrompu ou altéré. Le sous-dossier est purgé immédiatement, une alerte est journalisée et le worker bascule sur le chemin d'acquisition froide.

### Étape 2 : Acquisition froide (Téléchargement sécurisé)
1. Acquisition d'un verrou local non bloquant (`fs2::File::lock_exclusive` ou `tokio::sync::Mutex`) sur `.lock` pour empêcher les téléchargements concurrents par plusieurs threads du worker.
2. Vérification de la politique réseau :
   - Si `networkAtInstall: "offline-preseeded"` et l'archive est absente du répertoire local d'amorçage : rejet immédiat avec `AppError::ToolUnavailableOffline`.
   - Si `networkAtInstall: "explicit-allowlist"` : téléchargement HTTPS en flux vers un fichier temporaire unique dans `staging/`.
3. Pendant le streaming :
   - Limitation stricte de la taille maximale téléchargeable (protection contre les dénis de service par remplissage disque).
   - Calcul cumulatif de l'empreinte SHA-256 au fil de l'eau.
4. À la fin du téléchargement : comparaison de l'empreinte obtenue avec `archiveSha256`.
   - Si différent : suppression immédiate du fichier temporaire et émission d'une erreur d'intégrité `AppError::ChecksumMismatch`.

### Étape 3 : Extraction sécurisée native (`noShellUnpack: true`)
1. L'archive est ouverte directement en flux à l'aide des crates Rust `tar` et `flate2` (ou `xz2` pour les archives `.tar.xz`).
2. Pour chaque entrée de l'archive :
   - Assainissement du chemin : interdiction stricte de tout segment `..`, de tout séparateur Windows `\` ou de chemin absolu.
   - Vérification que la cible réside strictement dans le répertoire de destination (`canonicalize()` borné).
   - Rejet de tout lien symbolique (*symlink*) pointant en dehors de la racine de l'outil.
3. Écriture du binaire cible dans le répertoire `bin/` avec l'attribut exécutable `0755`.
4. Suppression du fichier d'archive temporaire dans `staging/`.

### Étape 4 : Validation terminale et finalisation
1. Calcul du hash SHA-256 du binaire extrait sur disque.
2. Vérification de conformité stricte avec `binarySha256`.
3. Écriture atomique du fichier `.sha256` et du fichier marqueur `.ready`.
4. Libération du verrou `.lock`. L'outil est prêt pour l'exécution.

---

## 4. Règles d'exécution et isolation des processus

L'exécution des binaires du cache respecte les règles du skill `rust-conventions-backend` et les impératifs de sécurité :

### 4.1 Invocation directe sans shell
L'exécution se fait exclusivement via l'API Tokio de processus :

```rust
let mut cmd = tokio::process::Command::new(&binary_path);
cmd.args(&sanitized_argv)
   .env_clear() // Isolation de l'environnement
   .envs(allowed_environment_variables)
   .current_dir(&sandbox_work_dir)
   .stdout(std::process::Stdio::piped())
   .stderr(std::process::Stdio::piped())
   .kill_on_drop(true);
```

- Aucun appel à `/bin/sh`, `/bin/bash` ou interpréteur de commande.
- Les arguments sont transmis individuellement sous forme d'un tableau `&[&str]` typé, empêchant toute injection de commande.

### 4.2 Chemins de fichiers opaques et sandboxés
- Les outils ne peuvent lire et écrire que dans les répertoires explicitement autorisés :
  - En entrée : les fichiers sources résidant dans `/work/jobs/{job_id}/input/` ou le cache source vérifié.
  - En sortie : les fichiers intermédiaires résidant dans `/work/jobs/{job_id}/scratch/`.
- Tout chemin passé en argument est validé canoniquement pour s'assurer qu'il ne traverse pas vers l'extérieur de la sandbox de la tâche.

### 4.3 Quotas d'exécution et limitation des fuites de ressources
- **Timeout strict** : chaque processus est enveloppé dans un délai d'attente borné (`tokio::time::timeout`), au-delà duquel un signal `SIGTERM` puis `SIGKILL` est envoyé via `kill_on_drop(true)`.
- **Limitation des flux stdout/stderr** : capture bornée à un volume fini (ex. 8 Mio maximum) pour éviter toute saturation mémoire du worker en cas de boucle de logs d'un outil défaillant.
