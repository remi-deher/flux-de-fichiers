# 0006 — Blocs génériques hautement paramétrables et exclusion de la duplication

Statut : accepté par l'utilisateur le 11 novembre 2026.

## Contexte et problématique

Lors de la modélisation des chaînes de traitement multimédia (FFmpeg et MKVToolNix), deux approches de granularité des blocs sont envisageables :

1. **L'approche par blocs spécialisés à l'extrême** :
   Créer un bloc distinct pour chaque permutation de paramètres (par exemple : un bloc `Video-H264-CPU`, un bloc `Video-HEVC-QSV-1080p`, un bloc `Audio-AAC-192k`, un bloc `Audio-AC3-5.1`, etc.).
   * *Inconvénients majeurs* : explosion combinatoire du catalogue, maintenance fastidieuse, duplication massive de schémas déclaratifs, et obligation de créer un nouveau bloc à chaque nouvelle variation de codec, d'encodeur ou de filtre.

2. **L'approche par blocs génériques paramétrables (style FileFlows)** :
   Définir un nombre restreint de blocs par grand domaine fonctionnel (Vidéo, Audio, Sous-titres, Métadonnées, Conteneur), chacun portant l'ensemble de ses options d'encodage configurables via des champs de saisie structurés et un champ d'arguments personnalisés libres.

## Décision utilisateur du 11 novembre 2026

L'utilisateur a validé l'adoption stricte de l'**approche par blocs génériques hautement paramétrables** :

### 1. Règle de zéro duplication de blocs
- **Aucune duplication de bloc** n'est tolérée pour une simple variation de paramètres (codec, encodeur matériel, débit, résolution, langue).
- Les options et variantes doivent être exposées comme des **paramètres configurables au sein de l'instance du bloc** (`fields` / `parameters`).

### 2. Composition standardisée d'un bloc multimédia
Chaque bloc déclaratif conforme au schéma `block.v1.1.schema.json` doit intégrer :
1. **Des sélecteurs de haut niveau conviviaux** :
   - Choix du codec et du format ;
   - Choix de l'encodeur matériel (`Automatic`, `Intel QSV`, `NVIDIA NVENC`, `VAAPI`, `CPU`, `AMD AMF`) ;
   - Curseurs intuitifs de compromis (ex. Qualité de 1 à 9, Vitesse de 1 à 5).
2. **Des valeurs par défaut saines** : garantissant une exécution immédiate sans saisie obligatoire pour l'utilisateur standard.
3. **Un champ d'arguments personnalisés libres (`custom_parameters` / `extra_args`)** :
   - Permet à l'utilisateur avancé d'injecter des drapeaux CLI supplémentaires (`ffmpeg` ou `mkvmerge`) sans modifier le catalogue ni développer de nouveau composant.

### 3. Cartographie des blocs fondamentaux retenus

| Domaine | Identifiant du bloc | Outil sous-jacent | Responsabilité principale |
| :--- | :--- | :--- | :--- |
| **Vidéo** | `block.video.encode` | `app.ffmpeg` | Encodage vidéo multi-codecs avec accélération matérielle et filtres de redimensionnement. |
| **Vidéo** | `block.video.inspect` | `app.ffmpeg` / `app.mkvtoolnix` | Sonde et inspection technique des pistes, résolutions, profils HDR et formats. |
| **Audio** | `block.audio.encode` | `app.ffmpeg` | Transcodage multi-formats, normalisation EBU R128 (`loudnorm`) et mixage de canaux. |
| **Audio** | `block.audio.extract` | `app.ffmpeg` / `app.mkvtoolnix` | Démultiplexage et extraction brute ou convertie de flux sonores. |
| **Sous-titres** | `block.subtitle.manage` | `app.mkvtoolnix` / `app.ffmpeg` | Filtrage de langues, extraction, bascule des drapeaux forcé/défaut et exclusion SDH. |
| **Conteneur** | `block.video.remux-mkv` | `app.mkvtoolnix` | Remuxage Matroska sans réencodage, conservation sélective de pistes et contrôle d'intégrité. |
| **Métadonnées** | `block.metadata.editor` | `app.mkvtoolnix` (`mkvpropedit`) | Modification *in-place* instantanée des titres, étiquettes de pistes et langues sans réécriture du fichier. |
| **Chapitres** | `block.chapters.manage` | `app.mkvtoolnix` | Importation, suppression et édition des repères de chapitres Matroska. |
| **Découpe/Fusion**| `block.media.split-concat` | `app.mkvtoolnix` / `app.ffmpeg` | Scission et assemblage de segments média sans perte. |

## Conséquences

- **Catalogue compact et lisible** : Moins de 10 blocs suffisent à couvrir l'intégralité des besoins de traitement multimédia.
- **Ergonomie calquée sur l'état de l'art** : Expérience utilisateur limpide inspirée de FileFlows.
- **Évolutivité sans régression** : Tout besoin d'encodage atypique est résolu immédiatement par les paramètres du bloc existant ou via le champ de paramètres personnalisés.
