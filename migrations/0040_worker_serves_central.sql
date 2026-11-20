-- Le worker intégré sert les dossiers du central comme un worker ordinaire (ADR 0022) : un seul
-- worker à la fois porte ce rôle, et les billets de lecture des sources du central le désignent.
ALTER TABLE workers ADD COLUMN serves_central INTEGER NOT NULL DEFAULT 0
    CHECK (serves_central IN (0, 1));
