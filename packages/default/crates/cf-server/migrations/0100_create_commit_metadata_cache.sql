-- Add commit_metadata_cache table for fast flakes view loading
-- Caches evaluation summary statistics to avoid expensive derivations joins

CREATE TABLE commit_metadata_cache (
    id SERIAL PRIMARY KEY,
    commit_id INTEGER NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    
    -- Evaluation summary statistics
    total_systems INTEGER NOT NULL DEFAULT 0,
    systems_passed_policy INTEGER NOT NULL DEFAULT 0,
    systems_failed_policy_strict INTEGER NOT NULL DEFAULT 0,
    systems_failed_policy_non_strict INTEGER NOT NULL DEFAULT 0,
    systems_with_eval_error INTEGER NOT NULL DEFAULT 0,
    
    -- Status classification flags
    has_nix_eval_error BOOLEAN NOT NULL DEFAULT FALSE,
    has_policy_failures BOOLEAN NOT NULL DEFAULT FALSE,
    all_systems_passed BOOLEAN NOT NULL DEFAULT FALSE,
    
    -- Cache metadata
    cached_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    -- Ensure one cache entry per commit
    UNIQUE(commit_id)
);

-- Index for fast lookup by commit_id
CREATE INDEX idx_commit_metadata_cache_commit_id ON commit_metadata_cache(commit_id);

-- Index for garbage collection queries (find old entries)
CREATE INDEX idx_commit_metadata_cache_cached_at ON commit_metadata_cache(cached_at);

-- Add comment explaining purpose
COMMENT ON TABLE commit_metadata_cache IS 'Caches evaluation summary statistics for fast flakes view loading. Populated after evaluation completes. Garbage collected after configurable retention period.';
