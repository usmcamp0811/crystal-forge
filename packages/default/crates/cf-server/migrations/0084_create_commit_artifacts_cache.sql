CREATE TABLE IF NOT EXISTS commit_artifacts_cache (
    commit_id integer PRIMARY KEY REFERENCES commits(id) ON DELETE CASCADE,
    nixos_configurations text[] NOT NULL DEFAULT ARRAY[]::text[],
    changed_files text[] NOT NULL DEFAULT ARRAY[]::text[],
    populated_at timestamptz NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_commit_artifacts_cache_populated_at
    ON commit_artifacts_cache (populated_at DESC);
