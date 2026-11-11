-- Gestion et persistance des flux de traitement (Flows) type Zapier / FileFlows
CREATE TABLE flows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    file_mask TEXT NOT NULL DEFAULT '*.mkv, *.mp4',
    is_default BOOLEAN NOT NULL DEFAULT 0,
    definition TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX flows_default_idx ON flows (is_default);

-- Flux 1 : Remux MKV par défaut
INSERT INTO flows (id, name, description, file_mask, is_default, definition) VALUES (
    'flow.default.mkv',
    'Remuxage Matroska propre',
    'Inspection préalable des métadonnées, filtrage des langues audio/sous-titres voulues et remuxage sans perte.',
    '*.mkv, *.avi, *.mp4',
    1,
    '{"nodes":[{"id":"node-1","type":"file-input","name":"Fichier source"},{"id":"node-2","type":"remux-mkv","name":"Remux MKV (MKVToolNix)","config":{"languages":["fre","eng"]}},{"id":"node-3","type":"file-output","name":"Destination finale","config":{"mode":"replace"}}],"connections":[{"from":"node-1","to":"node-2"},{"from":"node-2","to":"node-3"}]}'
);

-- Flux 2 : Optimisation Web MP4 FastStart
INSERT INTO flows (id, name, description, file_mask, is_default, definition) VALUES (
    'flow.remux.mp4',
    'Optimisation Web MP4 (FastStart)',
    'Convertit le conteneur en MP4 avec optimisation FastStart (moov atom en début de fichier) via MP4Box.',
    '*.mp4, *.m4v, *.mov',
    0,
    '{"nodes":[{"id":"node-1","type":"file-input","name":"Fichier source"},{"id":"node-2","type":"remux-mp4","name":"Remux MP4 (MP4Box)","config":{"faststart":true}},{"id":"node-3","type":"file-output","name":"Destination finale","config":{"mode":"replace"}}],"connections":[{"from":"node-1","to":"node-2"},{"from":"node-2","to":"node-3"}]}'
);

-- Flux 3 : Aiguillage conditionnel intelligent & Exécution In-Place (ADR-0008, ADR-0009)
INSERT INTO flows (id, name, description, file_mask, is_default, definition) VALUES (
    'flow.smart.adr0008',
    'Aiguillage conditionnel & In-Place (ADR-0008)',
    'Inspecte automatiquement le média : si déjà HEVC, applique les drapeaux en-têtes directement en place (mkvpropedit) sans copie ; sinon transcode par GPU avec repli CPU garanti.',
    '*.mkv, *.mp4, *.m2ts, *.ts',
    0,
    '{"nodes":[{"id":"node-1","type":"file-input","name":"Fichier source"},{"id":"node-2","type":"condition","name":"Condition : Codec == HEVC ?","config":{"property":"video.codec","operator":"eq","target":"hevc"}},{"id":"node-3","type":"mkvpropedit","name":"Édition En-têtes (In-Place)","config":{"default_audio_lang":"fre","force_subtitles":true}},{"id":"node-4","type":"video-encode","name":"Transcodage Vidéo HEVC","config":{"codec":"hevc","encoder":"qsv","quality":6}},{"id":"node-5","type":"file-output","name":"Destination & Notification","config":{"mode":"replace","notify_arr":true}}],"connections":[{"from":"node-1","to":"node-2"},{"from":"node-2","to":"node-3","port":"true"},{"from":"node-2","to":"node-4","port":"false"},{"from":"node-3","to":"node-5"},{"from":"node-4","to":"node-5"}]}'
);
