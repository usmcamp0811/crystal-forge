-- Migration 0203: Add trust state tracking for policy and bundle versions.
--
-- Enables explicit admin-authorized trust operations on policy and bundle versions.
-- Imported executable content defaults to untrusted; users must explicitly review
-- and approve before activation.

ALTER TABLE deployment_policy_versions
    ADD COLUMN IF NOT EXISTS trust_state text NOT NULL DEFAULT 'untrusted',
    ADD COLUMN IF NOT EXISTS trusted_by uuid REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS trusted_at timestamptz,
    ADD COLUMN IF NOT EXISTS trust_review_note text,
    ADD CONSTRAINT deployment_policy_versions_trust_state_valid
        CHECK (trust_state IN ('untrusted', 'trusted', 'rejected'));

ALTER TABLE compliance_bundle_versions
    ADD COLUMN IF NOT EXISTS trust_state text NOT NULL DEFAULT 'untrusted',
    ADD COLUMN IF NOT EXISTS trusted_by uuid REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS trusted_at timestamptz,
    ADD COLUMN IF NOT EXISTS trust_review_note text,
    ADD CONSTRAINT compliance_bundle_versions_trust_state_valid
        CHECK (trust_state IN ('untrusted', 'trusted', 'rejected'));

-- Backfill existing versions as untrusted (safe default for all imported content).
-- Published versions should be marked as trusted for backward compatibility when needed.
UPDATE deployment_policy_versions
SET trust_state = 'untrusted'
WHERE trust_state IS NULL;

UPDATE compliance_bundle_versions
SET trust_state = 'untrusted'
WHERE trust_state IS NULL;
