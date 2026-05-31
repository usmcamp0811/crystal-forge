-- Fix uniqueness semantics for hardening justifications when directive_name IS NULL.
--
-- PostgreSQL UNIQUE constraints treat NULLs as distinct, so the prior
-- (system_id, service_name, directive_name) constraint allowed duplicate
-- service-level justifications with directive_name = NULL.
--
-- Replace with partial unique indexes:
--   1) one service-level row per (system_id, service_name) when directive_name IS NULL
--   2) one directive-level row per (system_id, service_name, directive_name) when directive_name IS NOT NULL

-- Keep newest service-level justification row and remove older duplicates.
WITH ranked_null_rows AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY system_id, service_name
            ORDER BY updated_at DESC, created_at DESC, id DESC
        ) AS rn
    FROM hardening_justifications
    WHERE directive_name IS NULL
)
DELETE FROM hardening_justifications hj
USING ranked_null_rows ranked
WHERE hj.id = ranked.id
  AND ranked.rn > 1;

ALTER TABLE hardening_justifications
    DROP CONSTRAINT IF EXISTS uq_hardening_justification;

CREATE UNIQUE INDEX idx_hardening_justifications_service_level_unique
    ON hardening_justifications (system_id, service_name)
    WHERE directive_name IS NULL;

CREATE UNIQUE INDEX idx_hardening_justifications_directive_level_unique
    ON hardening_justifications (system_id, service_name, directive_name)
    WHERE directive_name IS NOT NULL;
