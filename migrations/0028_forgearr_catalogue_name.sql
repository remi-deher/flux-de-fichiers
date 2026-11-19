UPDATE plugin_repositories
SET name = 'Catalogue officiel Forgearr'
WHERE id = 'repo.official'
  AND is_official = 1
  AND name = 'Catalogue Officiel Flux de Fichiers';
