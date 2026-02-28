-- Migration 0083: Create builders infrastructure for multi-builder API support
-- This migration creates the tables needed for:
-- - Builder registration and management
-- - Builder-to-environment assignments (1:many)
-- - Builder resource metrics tracking
-- - Build job tracking with retry logic

-- =============================================================================
-- 1. CREATE BUILDERS TABLE
-- =============================================================================
CREATE TABLE IF NOT EXISTS builders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive', 'offline')) DEFAULT 'inactive',
    max_cpu_cores INTEGER,  -- NULL means unlimited
    max_memory_mb INTEGER,   -- NULL means unlimited
    max_concurrent_jobs INTEGER NOT NULL DEFAULT 1 CHECK (max_concurrent_jobs > 0),
    last_heartbeat_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for status queries (listing active builders)
CREATE INDEX idx_builders_status ON builders(status);

-- Index for heartbeat timeout detection
CREATE INDEX idx_builders_last_heartbeat ON builders(last_heartbeat_at) WHERE status != 'offline';

COMMENT ON TABLE builders IS 'Registered build workers that communicate via API';
COMMENT ON COLUMN builders.status IS 'active: registered and working, inactive: paused, offline: missed heartbeat';
COMMENT ON COLUMN builders.max_cpu_cores IS 'NULL means unlimited CPU cores';
COMMENT ON COLUMN builders.max_memory_mb IS 'NULL means unlimited memory';
COMMENT ON COLUMN builders.max_concurrent_jobs IS 'Number of builds this builder can run in parallel';

-- =============================================================================
-- 2. CREATE BUILDER ENVIRONMENT ASSIGNMENTS TABLE (1:many relationship)
-- =============================================================================
CREATE TABLE IF NOT EXISTS builder_environment_assignments (
    id SERIAL PRIMARY KEY,
    builder_id UUID NOT NULL REFERENCES builders(id) ON DELETE CASCADE,
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(builder_id, environment_id)
);

-- Index for querying which builders are assigned to an environment
CREATE INDEX idx_builder_env_environment_id ON builder_environment_assignments(environment_id);

-- Index for querying which environments a builder serves
CREATE INDEX idx_builder_env_builder_id ON builder_environment_assignments(builder_id);

COMMENT ON TABLE builder_environment_assignments IS 'Maps builders to environments (1:many). No assignments = wildcard (builds all)';

-- =============================================================================
-- 3. CREATE BUILDER METRICS TABLE
-- =============================================================================
CREATE TABLE IF NOT EXISTS builder_metrics (
    id BIGSERIAL PRIMARY KEY,
    builder_id UUID NOT NULL REFERENCES builders(id) ON DELETE CASCADE,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    cpu_usage_percent DOUBLE PRECISION NOT NULL CHECK (cpu_usage_percent >= 0 AND cpu_usage_percent <= 100),
    memory_usage_mb BIGINT NOT NULL CHECK (memory_usage_mb >= 0),
    system_cpu_usage_percent DOUBLE PRECISION CHECK (system_cpu_usage_percent >= 0 AND system_cpu_usage_percent <= 100),
    system_memory_total_mb BIGINT CHECK (system_memory_total_mb >= 0),
    system_memory_used_mb BIGINT CHECK (system_memory_used_mb >= 0)
);

-- Index for querying recent metrics by builder
CREATE INDEX idx_builder_metrics_builder_timestamp ON builder_metrics(builder_id, timestamp DESC);

-- Index for metrics cleanup/pruning queries
CREATE INDEX idx_builder_metrics_timestamp ON builder_metrics(timestamp);

COMMENT ON TABLE builder_metrics IS 'Resource usage metrics reported by builders via heartbeat';
COMMENT ON COLUMN builder_metrics.cpu_usage_percent IS 'Builder process CPU usage (0-100)';
COMMENT ON COLUMN builder_metrics.memory_usage_mb IS 'Builder process memory usage in MB';
COMMENT ON COLUMN builder_metrics.system_cpu_usage_percent IS 'System-wide CPU usage (0-100), optional';
COMMENT ON COLUMN builder_metrics.system_memory_total_mb IS 'Total system memory in MB, optional';
COMMENT ON COLUMN builder_metrics.system_memory_used_mb IS 'Used system memory in MB, optional';

-- =============================================================================
-- 4. CREATE BUILD JOBS TABLE (extends/replaces derivation tracking)
-- =============================================================================
-- This table tracks the assignment of derivations to specific builders
-- and manages the build queue with retry logic
CREATE TABLE IF NOT EXISTS build_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    builder_id UUID REFERENCES builders(id) ON DELETE SET NULL,  -- NULL = unassigned
    derivation_id INTEGER NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    environment_id UUID REFERENCES environments(id) ON DELETE SET NULL,  -- for filtering
    status TEXT NOT NULL CHECK (status IN ('queued', 'building', 'success', 'failed')) DEFAULT 'queued',
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    max_retries INTEGER NOT NULL DEFAULT 3 CHECK (max_retries >= 0),
    priority_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0 CHECK (priority_weight > 0),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    logs TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for job queue queries (find next job for builder)
CREATE INDEX idx_build_jobs_queue ON build_jobs(status, priority_weight DESC, created_at ASC)
    WHERE status = 'queued';

-- Index for active jobs by builder (concurrency tracking)
CREATE INDEX idx_build_jobs_builder_active ON build_jobs(builder_id)
    WHERE status = 'building';

-- Index for environment-based filtering
CREATE INDEX idx_build_jobs_environment ON build_jobs(environment_id);

-- Index for derivation lookup
CREATE INDEX idx_build_jobs_derivation ON build_jobs(derivation_id);

-- Index for status queries
CREATE INDEX idx_build_jobs_status ON build_jobs(status);

COMMENT ON TABLE build_jobs IS 'Build queue with builder assignment and retry logic';
COMMENT ON COLUMN build_jobs.builder_id IS 'NULL = unassigned, otherwise assigned to specific builder';
COMMENT ON COLUMN build_jobs.environment_id IS 'Used for environment-based builder filtering';
COMMENT ON COLUMN build_jobs.retry_count IS 'Current retry attempt (0 = first attempt)';
COMMENT ON COLUMN build_jobs.max_retries IS 'Maximum retries before permanent failure';
COMMENT ON COLUMN build_jobs.priority_weight IS 'Higher weight = higher priority in queue (newer commits weighted higher)';
COMMENT ON COLUMN build_jobs.logs IS 'Build logs streamed from builder';

-- =============================================================================
-- 5. CREATE BUILD RESERVATIONS TABLE
-- =============================================================================
-- This tracks which derivations are reserved by which builders
-- to prevent duplicate work
CREATE TABLE IF NOT EXISTS build_reservations (
    id SERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL,  -- For backward compatibility with existing builder
    builder_id UUID REFERENCES builders(id) ON DELETE CASCADE,  -- New FK for API builders
    derivation_id INTEGER NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    nixos_derivation_id INTEGER REFERENCES derivations(id) ON DELETE CASCADE,
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (now() + interval '30 minutes'),
    UNIQUE(derivation_id)
);

-- Index for cleanup of expired reservations
CREATE INDEX idx_build_reservations_expires ON build_reservations(expires_at);

-- Index for builder lookup
CREATE INDEX idx_build_reservations_builder ON build_reservations(builder_id);

COMMENT ON TABLE build_reservations IS 'Tracks active build reservations to prevent duplicate work';
COMMENT ON COLUMN build_reservations.worker_id IS 'Legacy worker identifier (for backward compatibility)';
COMMENT ON COLUMN build_reservations.builder_id IS 'New builder UUID (for API-based builders)';

-- =============================================================================
-- 6. ADD TRIGGER FOR UPDATED_AT TIMESTAMPS
-- =============================================================================
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_builders_updated_at
    BEFORE UPDATE ON builders
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_build_jobs_updated_at
    BEFORE UPDATE ON build_jobs
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- =============================================================================
-- 7. INITIAL DATA SETUP
-- =============================================================================
-- Optionally create a default builder for backward compatibility
-- This can be removed once all builders are registered via the API
COMMENT ON TABLE builders IS 'Run `INSERT INTO builders (name, public_key, status) VALUES (''default-builder'', ''<pubkey>'', ''active'')` to create initial builder';
