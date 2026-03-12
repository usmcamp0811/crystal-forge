-- Create many-to-many relationship between cache destinations and environments
-- This allows per-environment cache routing

CREATE TABLE IF NOT EXISTS cache_destination_environments (
    cache_destination_id INTEGER NOT NULL REFERENCES cache_destinations(id) ON DELETE CASCADE,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (cache_destination_id, environment_id)
);

-- Index for fast lookups by cache destination
CREATE INDEX IF NOT EXISTS idx_cache_dest_envs_cache 
    ON cache_destination_environments(cache_destination_id);

-- Index for fast lookups by environment
CREATE INDEX IF NOT EXISTS idx_cache_dest_envs_env 
    ON cache_destination_environments(environment_id);

-- Comments
COMMENT ON TABLE cache_destination_environments IS 
    'Many-to-many relationship between cache destinations and environments. Cache destinations with no environment assignments are considered global (available to all environments).';

COMMENT ON COLUMN cache_destination_environments.cache_destination_id IS 
    'Foreign key to cache_destinations table';

COMMENT ON COLUMN cache_destination_environments.environment_id IS 
    'Foreign key to environments table';

COMMENT ON COLUMN cache_destination_environments.created_at IS 
    'When this cache-environment assignment was created';
