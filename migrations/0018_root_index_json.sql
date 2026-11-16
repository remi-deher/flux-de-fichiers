-- L'index des dépôts de plugins vit désormais à la racine (index.json).
UPDATE plugin_repositories
SET index_url = 'https://raw.githubusercontent.com/remi-deher/flux-de-fichiers-plugins/dev/index.json'
WHERE id = 'repo.official'
  AND is_official = 1
  AND index_url IN (
      'https://raw.githubusercontent.com/remi-deher/flux-de-fichiers-plugins/dev/catalogue/index.json',
      'https://raw.githubusercontent.com/remi-deher/flux-de-fichiers-plugins/main/catalogue/index.json'
  );
