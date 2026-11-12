-- Le catalogue officiel est lu sur la branche dev du dépôt de plugins (type app, dossier apps/).
UPDATE plugin_repositories
SET index_url = 'https://raw.githubusercontent.com/remi-deher/flux-de-fichiers-plugins/dev/catalogue/index.json'
WHERE id = 'repo.official'
  AND is_official = 1
  AND index_url = 'https://raw.githubusercontent.com/remi-deher/flux-de-fichiers-plugins/main/catalogue/index.json';
