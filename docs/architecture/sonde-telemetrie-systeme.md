# Conception technique de la sonde système de télémétrie

Date : 9 novembre 2026. Statut : spécification d'architecture technique (ADR 0004).

## 1. Objectifs et rôle architectural

La **sonde système de télémétrie** (`app.internal.telemetry`) est l'unique outil interne intégré nativement dans le binaire applicatif `flux-de-fichiers` (coordinateur central et agents workers).

Ses responsabilités sont les suivantes :
1. **Mesure matérielle locale** : relever périodiquement et sans impact de performance l'état des ressources matérielles du nœud : processeur (CPU), mémoire vive (RAM), accélérateurs graphiques et de transcodage (GPU / DRM / NVENC / QuickSync) et stockage local (`DATA_DIR`, `WORK_DIR`, `CACHE_DIR`).
2. **Confinement de sécurité** : opérer sous le profil non-root `app` (UID 10001) sans aucune élévation de privilèges (`sudo`, `capabilities` super-utilisateur), sans monter de socket Docker (`/var/run/docker.sock`), et sans exécuter de sous-processus shell.
3. **Alimentation du scheduling distribué** : transmettre un rapport d'état synthétique et structuré au coordinateur afin de guider l'affectation prédictive des tâches selon les capacités et la charge réelle de chaque worker.

---

## 2. Métriques relevées et sources Linux

Pour garantir une portabilité maximale sur les architectures cibles (serveurs Linux x86_64 et NAS aarch64 comme TRUENAS / ARTEMIS) sans dépendances binaires externes instables, la sonde lit directement les pseudofichiers du noyau Linux (`/proc`, `/sys`) et interroge les appels système de stockage POSIX (`statvfs`).

### 2.1 Processeur (CPU)
- **Source** : `/proc/stat` et `/proc/loadavg`.
- **Champs extraits** :
  - `logical_cores` : nombre total de cœurs logiques (`u32`).
  - `usage_percent` : pourcentage de temps CPU actif non-idle calculé sur un intervalle glissant (`f32` borné entre `0.0` et `100.0`).
  - `load_average` : moyennes de charge système à 1 min, 5 min et 15 min (`[f32; 3]`).
  - `pressure_cpu` (optionnel) : métriques de saturation lues depuis `/proc/pressure/cpu` si le noyau Linux expose PSI (*Pressure Stall Information*).

### 2.2 Mémoire vive (RAM et Swap)
- **Source** : `/proc/meminfo`.
- **Champs extraits** :
  - `total_bytes` : mémoire vive totale disponible sur la machine (`u64`).
  - `available_bytes` : mémoire immédiatement utilisable sans forcer de swap (`u64`).
  - `used_bytes` : mémoire effectivement consommée (`total_bytes - available_bytes`).
  - `usage_percent` : ratio d'utilisation (`f32` borné entre `0.0` et `100.0`).
  - `swap_total_bytes` et `swap_used_bytes` : état du fichier d'échange (`u64`).

### 2.3 Accélérateurs matériels et GPU
- **Détection non intrusive** :
  - Nœuds de rendu génériques Linux : balayage en lecture seule de `/dev/dri/by-path/` et `/dev/dri/renderD*` (VA-API / Intel QuickSync / AMD AMF).
  - Périphériques NVIDIA : inspection de l'existence de `/proc/driver/nvidia/version` et des nœuds `/dev/nvidia*`.
  - Périphériques ARM / V4L2 : détection des dispositifs de transcodage matériel (`/dev/video*`, Rockchip / Raspberry Pi V4L2-M2M).
- **Règle d'absence** : si aucun accélérateur n'est accessible ou configuré, le champ `gpu` est explicitement positionné à `null` (aucun plantage, aucune erreur bloquante).
- **Champs extraits si présent** :
  - `device_kind` : type d'accélérateur (`vaapi`, `nvidia`, `qsv`, `v4l2m2m`, `other`).
  - `device_path` : chemin du nœud (`/dev/dri/renderD128`, etc.).
  - `vendor` : identifiant constructeur (`intel`, `nvidia`, `amd`, `unknown`).
  - `utilization_percent` : taux d'utilisation si disponible sans dépendance lourde, sinon `None`.

### 2.4 Stockage et volumes de travail
- **Source** : appel système standard `statvfs` via les bibliothèques POSIX Rust sur les points de montage réels (`DATA_DIR`, `WORK_DIR`, `CACHE_DIR`).
- **Champs extraits par point de montage** :
  - `mount_point` : nom canonique (`data`, `work`, `tools_cache`).
  - `total_bytes` : taille totale de la partition (`u64`).
  - `available_bytes` : espace restant allouable pour l'utilisateur `app` (`u64`).
  - `used_bytes` : espace actuellement occupé (`u64`).
  - `low_disk_space_warning` : booléen activé lorsque l'espace libre franchit le seuil minimal de sécurité (seuil par défaut : `5 Gio` ou `10 %`).

---

## 3. Modélisation Rust des structures de données

Les structures sont déclarées de manière stricte avec `serde`, sans `unwrap()` et avec validation sémantique.

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Rapport complet d'un instantané de télémétrie.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeTelemetrySnapshot {
    pub timestamp: DateTime<Utc>,
    pub node_id: uuid::Uuid,
    pub arch: NodeArchitecture,
    pub cpu: CpuMetrics,
    pub ram: RamMetrics,
    pub gpu: Option<GpuMetrics>,
    pub storage: StorageMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeArchitecture {
    X86_64,
    Aarch64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CpuMetrics {
    pub logical_cores: u32,
    pub usage_percent: f32,
    pub load_average: [f32; 3],
    pub psi_some_avg10: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RamMetrics {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
    pub swap_used_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GpuMetrics {
    pub device_kind: String,
    pub device_path: PathBuf,
    pub vendor: String,
    pub utilization_percent: Option<f32>,
    pub memory_available_bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageMetrics {
    pub volumes: Vec<VolumeUsage>,
    pub has_critical_pressure: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VolumeUsage {
    pub role: String, // "data", "work", "tools_cache"
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
}
```

---

## 4. Intégration au protocole Coordinateur <-> Worker

L'intégration de la télémétrie s'insère dans les flux réseau existants sans créer de complexité superflue :

```
┌─────────────────────────────────────────────────────────┐
│                      Worker Node                        │
│  ┌────────────────────────┐                             │
│  │ Sondes matérielles     │                             │
│  │ (CPU, RAM, GPU, Disque)│                             │
│  └───────────┬────────────┘                             │
│              │ relevé (toutes les 20s)                  │
│              ▼                                          │
│  ┌────────────────────────┐                             │
│  │ Envoi Heartbeat / Reg. │                             │
└──────────────┬──────────────────────────────────────────┘
               │ HTTPS + Token Agent
               ▼
┌─────────────────────────────────────────────────────────┐
│                  Coordinateur Central                   │
│  ┌────────────────────────┐                             │
│  │ Ingestion & Cache État │                             │
│  │ (table / mémoire vive) │                             │
│  └───────────┬────────────┘                             │
│              │ données d'ordonnancement                 │
│              ▼                                          │
│  ┌───────────────────────────────────────────────────┐  │
│  │ Planificateur (Scheduler)                         │  │
│  │ - Filtrage des workers admissibles (CPU, RAM, GPU)│  │
│  │ - Attribution atomique FOR UPDATE SKIP LOCKED     │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### 4.1 Enregistrement (`POST /api/v1/workers`)
Lors de son premier enregistrement ou après redémarrage, le worker déclare sa fiche statique et son profil matériel :
- Architecture (`x86_64`, `aarch64`) ;
- Capacités d'accélération matérielle disponibles ;
- Emplacement et taille de son cache d'outils local.

### 4.2 Signal de vie périodique (`POST /api/v1/workers/{id}/heartbeat`)
Toutes les 20 secondes (pendant le renouvellement du bail de travail ou en veille), le worker transmet son dernier `NodeTelemetrySnapshot` :
- Le coordinateur met à jour la table PostgreSQL `workers` (ou une table d'historique court `node_telemetry`) avec les valeurs instantanées.
- Si le heartbeat d'un worker expire (absence de signal pendant plus de 120 secondes), le worker est considéré comme hors ligne et ses baux sont libérés pour réattribution.

---

## 5. Algorithme d'ordonnancement (Scheduler) du coordinateur

Le coordinateur exploite les métriques rapportées pour décider de l'attribution des tâches :

1. **Filtres d'éligibilité durs** :
   - *Architecture compatible* : si la tâche exige un outil distribué disponible uniquement pour `x86_64`, les workers `aarch64` sont exclus.
   - *Accélérateur requis* : si un macro-bloc spécifie une accélération GPU matérielle, seuls les nœuds avec `gpu.is_some()` sont retenus.
   - *Espace disque suffisant* : si `storage.available_bytes < (source_size * 2 + marge_securite)`, le nœud est temporairement écarté pour éviter un crash `NoSpaceLeftOnDevice`.
   - *Mémoire disponible* : la RAM disponible doit excéder le seuil alloué au job.
2. **Score de charge pondéré** :
   Pour chaque worker éligible, un score de disponibilité est calculé :
   $$\text{Score} = (100 - \text{cpu\_percent}) \times W_{\text{cpu}} + (100 - \text{ram\_percent}) \times W_{\text{ram}} + S_{\text{cache\_chaud}}$$
   où $S_{\text{cache\_chaud}}$ attribue un bonus si le worker possède déjà la version exacte de l'outil dans son cache local.
3. **Sélection atomique** :
   La réservation de la tâche reste garantie par la requête transactionnelle `SELECT ... FOR UPDATE SKIP LOCKED` existante, garantissant l'absence de collision entre nœuds concurrents.
