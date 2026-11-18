-- Questions posées à l'assistant IA par jour et par compte (quotas, ADR 0015).
CREATE TABLE IF NOT EXISTS ai_usage (
    day TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    requests INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, owner_id)
);
