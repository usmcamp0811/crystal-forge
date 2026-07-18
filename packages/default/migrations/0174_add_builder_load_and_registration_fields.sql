-- Migration 0174: Add builder load_avg and registration status fields
-- TASK-393: Builders design delta - side panel and open-flow parity
--
-- This migration adds:
-- - registered: whether builder has completed bootstrap handshake
-- - load_avg: current load average (0.0 - 1.0)
--
-- public_key_fingerprint is NOT stored as a column; it is computed from
-- public_key in queries using encode(digest(decode(public_key,'base64'),'sha256'),'hex').
-- This avoids synchronization issues between stored and actual fingerprints.

-- =============================================================================
-- 1. ENABLE PGCRYPTO EXTENSION (for SHA256 hashing in queries)
-- =============================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =============================================================================
-- 2. ADD NEW COLUMNS
-- =============================================================================

-- Whether builder has completed bootstrap/registration (starts true for existing flow)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS registered BOOLEAN NOT NULL DEFAULT true;

-- Current load average 0.0-1.0 (nullable, updated via heartbeat/metrics)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS load_avg DOUBLE PRECISION
    CHECK (load_avg IS NULL OR (load_avg >= 0.0 AND load_avg <= 1.0))
    DEFAULT NULL;

-- =============================================================================
-- 3. ADD COMMENTS
-- =============================================================================

COMMENT ON COLUMN builders.registered IS 'Whether builder has completed bootstrap handshake (default true for currently existing flow; reserved for future pending-builder state)';
COMMENT ON COLUMN builders.load_avg IS 'System load average (0.0-1.0), derived from latest builder_metrics system_cpu_usage_percent, NULL if not yet reported';
