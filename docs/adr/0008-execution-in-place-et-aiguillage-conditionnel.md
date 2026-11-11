# ADR 0008 : Exécution in-place non destructive, cycle WorkingFile et aiguillage conditionnel multi-sorties

## Contexte

Dans les architectures de traitement de flux multimédias (telles que celle de FileFlows), la règle habituelle d'isolation impose de ne jamais toucher au fichier source et de travailler dans un répertoire temporaire dédié avant remplacement final.
Cependant, pour les fichiers volumineux (films ou séries de 5 à 40 Go), une opération légère comme l'édition de métadonnées d'en-tête (changement de titre de piste de sous-titres, modification des drapeaux `default` ou `forced` via `mkvpropedit`) entraînerait une copie complète inutile de tout le fichier, gaspillant des entrées/sorties disque et ralentissant considérablement le traitement.

Par ailleurs, l'introduction de flux intelligents nécessite d'aiguiller dynamiquement le traitement selon les caractéristiques du fichier (ex: sauter l'encodage si la vidéo est déjà en HEVC, ou basculer en CPU si le GPU échoue).

## Décisions

1. **Dualité d'exécution : In-Place vs Espace de staging isolé**
   - **Opérations légères d'en-têtes (In-Place)** : Les opérations non destructrices reconnues (notamment `mkvpropedit` sur les en-têtes EBML Matroska) s'exécutent directement en place sur le fichier, sans copie préalable dans `/work`.
   - **Opérations structurelles ou de transformation (WorkingFile isolé)** : Les opérations lourdes (réencodage FFmpeg, remuxage de pistes mkvmerge, repackaging MP4Box) s'exécutent strictement dans le sous-dossier éphémère `/work/job-<uuid>/`. Le pointeur `working_file` évolue d'étape en étape, et le fichier temporaire précédent est automatiquement détruit dès validation de l'étape.

2. **Finalisation et atomicité**
   - Le mode *Replace Original* effectue un renommage atomique POSIX Unix (`rename`) du `working_file` final vers l'emplacement d'origine avec restauration des métadonnées `mtime` et des droits de propriété.
   - En cas d'erreur ou d'annulation, le sous-dossier `/work/job-<uuid>/` est immédiatement purgé et le fichier d'origine demeure intact.

3. **Inspection implicite dès l'entrée**
   - MediaInfo CLI et FFprobe extraient les métadonnées techniques dès l'interception du fichier.
   - Ces informations sont injectées dans le contexte du job (`media_info`), éliminant tout bloc visuel redondant d'inspection.

4. **Aiguillage conditionnel à trois sorties**
   - Les blocs de décision conditionnelle exposent trois sorties distinctes :
     - **Sortie 1 (Vrai / Match)** : La condition est vérifiée (ex: déjà HEVC ➔ saut vers le remuxage).
     - **Sortie 2 (Faux / Else)** : La condition n'est pas vérifiée (ex: H.264 ➔ routage vers l'encodeur GPU).
     - **Sortie 3 (Repli / Fallback)** : En cas d'incompatibilité matérielle ou d'échec du transcodeur matériel GPU (Intel QSV / NVENC), bascule automatique sur l'encodeur CPU logiciel (`libx265`).

## Statut

Accepté et validé par décision utilisateur du 11 novembre 2026.
