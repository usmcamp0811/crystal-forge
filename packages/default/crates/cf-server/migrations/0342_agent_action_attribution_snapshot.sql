-- Durable agent attribution on POA&M history (TASK-534.5 review).
--
-- Migration 0341 records `access_token_id` with ON DELETE SET NULL, and the
-- POA&M activity and verification views read the token name from the live
-- token row. This migration stores the attribution on the history row itself:
--
-- - `via_agent` is true when an access token made the change. It does not
--   depend on the token foreign key.
-- - `access_token_name` is the token name at write time.
--
-- Existing rows are not updated: the immutability triggers on both tables
-- reject UPDATE. Readers treat `access_token_id IS NOT NULL` as `via_agent`
-- and fall back to the live token name when `access_token_name` is NULL.
--
-- Token deletion: the SET NULL action of 0341 is an UPDATE of the referencing
-- history row, so `trigger_prevent_poam_activity_mutation` and
-- `trigger_protect_poam_verification_attempt` reject the DELETE of a token
-- that has POA&M activity or attempts. These columns keep the attribution if
-- that rule changes.
--
-- A constant default makes ADD COLUMN a catalog-only change. It does not
-- rewrite rows or fire row triggers.

ALTER TABLE poam_activity
    ADD COLUMN IF NOT EXISTS via_agent boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS access_token_name text;

ALTER TABLE poam_verification_attempts
    ADD COLUMN IF NOT EXISTS via_agent boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS access_token_name text;
