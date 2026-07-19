-- Migration 0124: Add builder UI display fields
-- This migration adds fields required for the Builders view UI:
-- - host: SSH endpoint for the builder
-- - arch: system architecture (x86_64-linux, aarch64-linux, etc.)
-- - enabled: whether builder accepts new jobs (separate from status)
-- - Additional computed metrics will come from queries/views

-- =============================================================================
-- 1. ADD NEW COLUMNS TO BUILDERS TABLE
-- =============================================================================

-- SSH endpoint / hostname for the builder
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS host TEXT;

-- System architecture (x86_64-linux, aarch64-linux, aarch64-darwin, x86_64-darwin)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS arch TEXT
    CHECK (arch IN ('x86_64-linux', 'aarch64-linux', 'aarch64-darwin', 'x86_64-darwin'));

-- Whether builder is enabled to accept new jobs (independent of status)
-- A builder can be active (running) but not enabled (not accepting new work)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true;

COMMENT ON COLUMN builders.host IS 'SSH endpoint or hostname for the builder (e.g. hydra-01.production.cf.internal)';
COMMENT ON COLUMN builders.arch IS 'System architecture: x86_64-linux, aarch64-linux, aarch64-darwin, or x86_64-darwin';
COMMENT ON COLUMN builders.enabled IS 'Whether this builder accepts new build jobs (independent of status)';

-- =============================================================================
-- 2. UPDATE STATUS ENUM TO MATCH JSX REQUIREMENTS
-- =============================================================================
-- JSX uses: running, paused, offline, draining
-- Current: active, inactive, offline
-- We'll keep the current values in DB but map them in the application layer:
-- - active → running
-- - inactive → paused
-- - offline → offline
-- Add 'draining' status for graceful shutdown

ALTER TABLE builders
    DROP CONSTRAINT IF EXISTS builders_status_check;

ALTER TABLE builders
    ADD CONSTRAINT builders_status_check
    CHECK (status IN ('active', 'inactive', 'offline', 'draining'));

COMMENT ON COLUMN builders.status IS 'active=running, inactive=paused, offline=missed heartbeat, draining=graceful shutdown';

-- =============================================================================
-- 3. ADD INDEX FOR ARCHITECTURE FILTERING
-- =============================================================================
CREATE INDEX IF NOT EXISTS idx_builders_arch ON builders(arch) WHERE arch IS NOT NULL;

-- =============================================================================
-- 4. BACKFILL DEFAULTS FOR EXISTING BUILDERS
-- =============================================================================
-- Set default arch for any existing builders (most common case)
UPDATE builders SET arch = 'x86_64-linux' WHERE arch IS NULL;

-- Make arch NOT NULL after backfill
ALTER TABLE builders
    ALTER COLUMN arch SET NOT NULL;
