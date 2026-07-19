-- Classification banner server setting.
-- Single-row table (constrained to id = 1) storing the global DoD/CNSS
-- classification banner configuration persisted by the admin console.
CREATE TABLE IF NOT EXISTS classification_banner_config (
    id            int PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    enabled       boolean NOT NULL DEFAULT false,
    level         text    NOT NULL DEFAULT 'UNCLASSIFIED',
    custom_text   text    NOT NULL DEFAULT '',
    updated_at    timestamptz NOT NULL DEFAULT NOW()
);

-- Seed the single row so GET always returns a value without requiring a prior PUT.
INSERT INTO classification_banner_config (id, enabled, level, custom_text)
VALUES (1, false, 'UNCLASSIFIED', '')
ON CONFLICT (id) DO NOTHING;
