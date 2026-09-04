-- Allow an unavailable evaluation snapshot to carry a durable diagnostic.
--
-- Snapshot capture can fail while the underlying Nix configuration evaluation
-- succeeds. Migration 0245 restricted `error` to lifecycle 'failed', which left
-- an 'unavailable' snapshot unable to explain itself. Operators could therefore
-- not distinguish snapshot extraction failure from a snapshot that exceeded the
-- content size limit.
--
-- Lifecycle semantics are unchanged and remain authoritative:
--   available   a valid persisted snapshot, including a legitimate zero-option
--               snapshot
--   unavailable the Nix evaluation succeeded, but no trustworthy configuration
--               snapshot artifact could be produced
--   failed      the underlying Nix configuration evaluation itself failed
--
-- INVARIANT: `error` explains why THIS snapshot artifact is not available. For
-- lifecycle 'unavailable' it must never be written so that it implies the Nix
-- system evaluation failed. Callers redact the value before the first write.

DO $$
DECLARE
    existing_constraint text;
BEGIN
    -- 0245 created this constraint without an explicit name, so resolve the
    -- generated name instead of assuming PostgreSQL's numbering.
    SELECT con.conname INTO existing_constraint
    FROM pg_constraint con
    WHERE con.conrelid = 'evaluation_snapshots'::regclass
      AND con.contype = 'c'
      AND pg_get_constraintdef(con.oid) LIKE '%error IS NULL%';

    IF existing_constraint IS NULL THEN
        RAISE EXCEPTION
            'evaluation_snapshots error-lifecycle CHECK constraint not found';
    END IF;

    EXECUTE format(
        'ALTER TABLE evaluation_snapshots DROP CONSTRAINT %I',
        existing_constraint
    );
END
$$;

ALTER TABLE evaluation_snapshots
    ADD CONSTRAINT evaluation_snapshots_error_lifecycle_check
    CHECK (lifecycle IN ('failed', 'unavailable') OR error IS NULL);
