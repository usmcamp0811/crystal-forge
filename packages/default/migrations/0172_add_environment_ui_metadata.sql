ALTER TABLE environments
    ADD COLUMN IF NOT EXISTS default_policy TEXT NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS auto_sync BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_production BOOLEAN NOT NULL DEFAULT FALSE;

UPDATE environments
SET default_policy = CASE
        WHEN LOWER(name) IN ('development', 'dev') THEN 'auto_latest'
        WHEN LOWER(name) IN ('remote', 'lab', 'lan') THEN 'pinned'
        ELSE 'manual'
    END,
    auto_sync = CASE
        WHEN LOWER(name) IN ('remote', 'lab', 'lan') THEN FALSE
        ELSE TRUE
    END,
    requires_approval = CASE
        WHEN LOWER(name) IN ('production', 'staging') THEN TRUE
        ELSE FALSE
    END,
    is_production = CASE
        WHEN LOWER(name) = 'production' THEN TRUE
        ELSE FALSE
    END;
