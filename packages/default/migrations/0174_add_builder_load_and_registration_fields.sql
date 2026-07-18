-- Migration 0174: Add builder load_avg and registration status fields
-- TASK-393: Builders design delta - side panel and open-flow parity
--
-- This migration adds:
-- - public_key_fingerprint: hex-encoded SHA256 fingerprint for display
-- - registered: whether builder has completed bootstrap handshake
-- - load_avg: current load average (0.0 - 100.0+)

-- =============================================================================
-- 1. ADD NEW COLUMNS
-- =============================================================================

-- SHA256 hex fingerprint of public key for UI display (32 bytes = 64 hex chars)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS public_key_fingerprint TEXT;

-- Whether builder has completed bootstrap/registration (starts false)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS registered BOOLEAN NOT NULL DEFAULT false;

-- Current load average percentage (nullable, updated via heartbeat)
ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS load_avg DOUBLE PRECISION
    CHECK (load_avg IS NULL OR (load_avg >= 0));

-- =============================================================================
-- 2. ADD COMMENTS
-- =============================================================================

COMMENT ON COLUMN builders.public_key_fingerprint IS 'SHA256 hex fingerprint of public_key for UI display (64 hex chars)';
COMMENT ON COLUMN builders.registered IS 'Whether builder has completed bootstrap handshake';
COMMENT ON COLUMN builders.load_avg IS 'Current load average percentage (0.0-100.0+), NULL if not reported';

-- =============================================================================
-- 3. BACKFILL FINGERPRINTS FOR EXISTING BUILDERS
-- =============================================================================
-- Compute SHA256 fingerprints for existing builders with public keys
-- Note: This uses PostgreSQL's pgcrypto extension which should already be enabled

-- Generate fingerprints from existing public_key base64 strings
-- We decode base64 -> bytes, hash with SHA256, then encode as hex
UPDATE builders
SET public_key_fingerprint = encode(digest(decode(public_key, 'base64'), 'sha256'), 'hex')
WHERE public_key IS NOT NULL
  AND public_key_fingerprint IS NULL;

-- =============================================================================
-- 4. CREATE INDEX FOR FINGERPRINT LOOKUPS
-- =============================================================================
CREATE INDEX IF NOT EXISTS idx_builders_fingerprint
    ON builders(public_key_fingerprint)
    WHERE public_key_fingerprint IS NOT NULL;
