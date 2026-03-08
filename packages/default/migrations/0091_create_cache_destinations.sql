-- Create cache_destinations table to store binary cache configurations
-- This allows dynamic cache management through the UI instead of server.toml only

CREATE TABLE IF NOT EXISTS cache_destinations (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    cache_type TEXT NOT NULL CHECK (cache_type IN ('S3', 'Attic', 'Http', 'Nix')),
    
    -- Common fields
    push_to TEXT, -- Destination URL (e.g., s3://bucket-name, https://cache.example.com)
    enabled BOOLEAN NOT NULL DEFAULT true,
    signing_key_path TEXT,
    compression TEXT, -- e.g., 'xz', 'zstd'
    
    -- S3-specific fields
    s3_region TEXT,
    s3_profile TEXT,
    
    -- Attic-specific fields
    attic_token TEXT, -- Auth token for Attic cache
    attic_cache_name TEXT, -- Cache name in Attic
    attic_ignore_upstream_cache_filter BOOLEAN DEFAULT true,
    attic_jobs INTEGER DEFAULT 5,
    
    -- Performance tuning
    parallel_uploads INTEGER DEFAULT 1,
    max_retries INTEGER DEFAULT 3,
    retry_delay_seconds BIGINT DEFAULT 5,
    push_timeout_seconds BIGINT DEFAULT 3600, -- 1 hour default
    
    -- Push behavior
    force_repush BOOLEAN DEFAULT false, -- Equivalent to --refresh flag
    require_sigs BOOLEAN DEFAULT true,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ -- Updated when cache worker uses this destination
);

-- Index for fast enabled lookup
CREATE INDEX IF NOT EXISTS idx_cache_destinations_enabled ON cache_destinations(enabled) WHERE enabled = true;

-- Index for cache type filtering
CREATE INDEX IF NOT EXISTS idx_cache_destinations_type ON cache_destinations(cache_type);

-- Trigger to update updated_at on modifications
CREATE OR REPLACE FUNCTION update_cache_destinations_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_cache_destinations_updated_at
    BEFORE UPDATE ON cache_destinations
    FOR EACH ROW
    EXECUTE FUNCTION update_cache_destinations_updated_at();
