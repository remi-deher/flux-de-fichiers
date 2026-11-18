-- Portée granulaire optionnelle des clés d'API (NULL = toutes les permissions du rôle).
ALTER TABLE api_keys ADD COLUMN scopes TEXT;
