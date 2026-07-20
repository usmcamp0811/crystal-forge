-- Migration 0175: Fix builder registration default and load_avg constraint
-- TASK-393: Builders design delta - side panel and open-flow parity
--
-- Migration 0174 added initial columns but had several issues:
--   - public_key_fingerprint: stored column causes sync issues with public_key
--   - registered: default false doesn't match existing registration model
--   - load_avg: CHECK allowed >= 0 but range should be 0.0-1.0
--
-- This migration fixes those issues:
--   1. Drop public_key_fingerprint column (computed from public_key in queries)
--   2. Set registered default to true, update existing rows
--   3. Recreate load_avg with correct 0.0-1.0 constraint

-- =============================================================================
-- 1. DROP public_key_fingerprint COLUMN AND INDEX
-- =============================================================================
-- Fingerprint is now computed from public_key in list_builders query using
-- encode(digest(decode(public_key,'base64'),'sha256'::text),'hex')
-- This avoids synchronization issues between stored and actual fingerprints.

DROP INDEX IF EXISTS idx_builders_fingerprint;
ALTER TABLE builders DROP COLUMN IF EXISTS public_key_fingerprint;

-- =============================================================================
-- 2. FIX registered DEFAULT AND UPDATE EXISTING ROWS
-- =============================================================================
-- All existing builders are considered registered by definition.
-- The 'unregistered' state is reserved for a future pending-builder approval flow.

ALTER TABLE builders ALTER COLUMN registered SET DEFAULT true;
UPDATE builders SET registered = true WHERE registered = false;

-- =============================================================================
-- 3. RECREATE load_avg WITH CORRECT CONSTRAINT
-- =============================================================================
-- load_avg should be 0.0-1.0 (matching Rust model docs). UI multiplies by 100
-- for display. If the column was created by 0174 with the old >= 0 constraint,
-- we drop and recreate it with the correct constraint.

ALTER TABLE builders DROP COLUMN IF EXISTS load_avg;
ALTER TABLE builders ADD COLUMN IF NOT EXISTS load_avg DOUBLE PRECISION
    CHECK (load_avg IS NULL OR (load_avg >= 0.0 AND load_avg <= 1.0))
    DEFAULT NULL;

-- =============================================================================
-- 4. UPDATE COMMENTS
-- =============================================================================

COMMENT ON COLUMN builders.registered IS 'Whether builder has completed bootstrap handshake (default true; reserved for future pending-builder state)';
COMMENT ON COLUMN builders.load_avg IS 'System load average (0.0-1.0), derived from latest builder_metrics system_cpu_usage_percent, NULL if not yet reported';
