-- Catalogue 2.0.0 : les identifiants de plugins perdent leur préfixe de type (`plugin.sonarr` →
-- `sonarr`, `plugin.core` → `essentiel`, `plugin.subtitles` → `sous-titres`). Les réglages
-- existants suivent leur plugin ; une ligne déjà enregistrée sous le nouvel identifiant est
-- conservée (OR IGNORE). Les secrets sont rescellés au démarrage (plugins::legacy_ids).
UPDATE OR IGNORE plugin_settings
SET plugin_id = CASE plugin_id
    WHEN 'plugin.core' THEN 'essentiel'
    WHEN 'plugin.subtitles' THEN 'sous-titres'
    ELSE substr(plugin_id, length('plugin.') + 1)
END
WHERE plugin_id IN (
    'plugin.core', 'plugin.video', 'plugin.audio', 'plugin.subtitles', 'plugin.scripts',
    'plugin.notifications', 'plugin.sonarr', 'plugin.radarr', 'plugin.plex', 'plugin.jellyfin',
    'plugin.emby'
);
