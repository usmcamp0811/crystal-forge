-- Dependency-plan totals must not reuse legacy closure accounting columns.
-- Only complete values produced after migration 0245 have the new semantics.
ALTER TABLE derivations
    ADD COLUMN dependency_derivation_count integer,
    ADD COLUMN dependency_build_plan_generation bigint NOT NULL DEFAULT 0,
    ADD COLUMN dependency_build_plan_lease_expires_at timestamptz,
    ADD COLUMN dependency_build_plan_legacy_generation bigint;

UPDATE derivations
SET dependency_derivation_count = closure_total,
    dependency_build_plan_generation = 1
WHERE dependency_build_plan_status = 'complete'
  AND closure_total IS NOT NULL
  AND dependency_build_count IS NOT NULL;

-- Preserve in-flight 0245 calculations under generation 1. The compatibility
-- trigger below accepts only the matching legacy terminal write. Recovery can
-- replace an abandoned calculation after its lease expires.
UPDATE derivations
SET dependency_derivation_count = NULL,
    dependency_build_count = NULL,
    dependency_build_plan_generation = 1,
    dependency_build_plan_lease_expires_at = NOW() + INTERVAL '10 minutes',
    dependency_build_plan_legacy_generation = 1
WHERE dependency_build_plan_status = 'calculating';

-- Existing queued jobs must remain claimable during a rolling upgrade. Other
-- non-complete plans have no trustworthy estimate and restart as unavailable.
UPDATE derivations
SET dependency_build_plan_status = CASE
        WHEN EXISTS (
            SELECT 1 FROM build_jobs bj WHERE bj.derivation_id = derivations.id
        ) THEN 'failed'
        ELSE 'unavailable'
    END,
    dependency_derivation_count = NULL,
    dependency_build_count = NULL,
    dependency_build_plan_generation = CASE
        WHEN EXISTS (
            SELECT 1 FROM build_jobs bj WHERE bj.derivation_id = derivations.id
        ) THEN 1
        ELSE 0
    END,
    dependency_build_plan_lease_expires_at = NULL,
    dependency_build_plan_legacy_generation = NULL
WHERE dependency_build_plan_status NOT IN ('complete', 'calculating');

-- Keep the legacy columns for compatibility, but remove them from the new
-- dependency-plan contract.
ALTER TABLE derivations
    DROP CONSTRAINT derivations_closure_total_nonnegative,
    DROP CONSTRAINT derivations_dependency_build_plan_complete_total,
    DROP CONSTRAINT derivations_dependency_build_count_within_total;

UPDATE derivations
SET closure_total = NULL,
    closure_cached = NULL;

-- COMPATIBILITY: A server from the 0245 release can remain active while this
-- migration runs. Translate its state-only writes into the 0246 contract until
-- a later release can remove this trigger after all 0245 servers are drained.
CREATE FUNCTION normalize_legacy_dependency_build_plan_write() RETURNS trigger AS $$
BEGIN
    IF NEW.dependency_build_plan_status = 'calculating'
       AND NEW.dependency_build_plan_generation = OLD.dependency_build_plan_generation
       AND NEW.dependency_build_plan_lease_expires_at IS NOT DISTINCT FROM OLD.dependency_build_plan_lease_expires_at
    THEN
        IF OLD.dependency_build_plan_status = 'calculating' THEN
            RETURN OLD;
        END IF;

        NEW.dependency_derivation_count := NULL;
        NEW.dependency_build_plan_generation := GREATEST(
            OLD.dependency_build_plan_generation + 1,
            1
        );
        NEW.dependency_build_plan_lease_expires_at := NOW() + INTERVAL '10 minutes';
        NEW.dependency_build_plan_legacy_generation := NEW.dependency_build_plan_generation;
    ELSIF NEW.dependency_build_plan_status = 'calculating' THEN
        -- A 0246 writer supplies a new generation and lease explicitly.
        NEW.dependency_build_plan_legacy_generation := NULL;
    ELSIF NEW.dependency_build_plan_status = 'complete'
          AND (
              NEW.closure_total IS DISTINCT FROM OLD.closure_total
              OR NEW.dependency_build_count IS DISTINCT FROM OLD.dependency_build_count
          )
          AND NEW.dependency_derivation_count IS NOT DISTINCT FROM OLD.dependency_derivation_count
    THEN
        IF OLD.dependency_build_plan_status = 'calculating'
           AND OLD.dependency_build_plan_legacy_generation = OLD.dependency_build_plan_generation
        THEN
            NEW.dependency_derivation_count := NEW.closure_total;
            NEW.dependency_build_plan_lease_expires_at := NULL;
            NEW.dependency_build_plan_legacy_generation := NULL;
        ELSE
            -- A 0245 terminal write has no generation token. Ignore it after
            -- a 0246 writer supersedes the legacy generation.
            RETURN OLD;
        END IF;
    ELSIF NEW.dependency_build_plan_status = 'failed'
          AND NEW.dependency_build_plan_lease_expires_at IS NOT DISTINCT FROM OLD.dependency_build_plan_lease_expires_at
    THEN
        IF OLD.dependency_build_plan_status = 'calculating'
           AND OLD.dependency_build_plan_legacy_generation = OLD.dependency_build_plan_generation
        THEN
            NEW.dependency_derivation_count := NULL;
            NEW.dependency_build_plan_lease_expires_at := NULL;
            NEW.dependency_build_plan_legacy_generation := NULL;
        ELSE
            RETURN OLD;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER normalize_legacy_dependency_build_plan_write
BEFORE UPDATE OF
    closure_total,
    dependency_build_count,
    dependency_build_plan_status
ON derivations
FOR EACH ROW
EXECUTE FUNCTION normalize_legacy_dependency_build_plan_write();

ALTER TABLE derivations
    ADD CONSTRAINT derivations_dependency_derivation_count_nonnegative
        CHECK (
            dependency_derivation_count IS NULL
            OR dependency_derivation_count >= 0
        ),
    ADD CONSTRAINT derivations_dependency_derivation_count_complete
        CHECK (
            (dependency_build_plan_status = 'complete')
            = (dependency_derivation_count IS NOT NULL)
        ),
    ADD CONSTRAINT derivations_dependency_build_count_within_derivation_count
        CHECK (
            dependency_build_count IS NULL
            OR dependency_build_count <= dependency_derivation_count
        ),
    ADD CONSTRAINT derivations_dependency_build_plan_generation_state
        CHECK (
            (
                dependency_build_plan_status = 'unavailable'
                AND dependency_build_plan_generation = 0
            )
            OR (
                dependency_build_plan_status IN ('calculating', 'complete', 'failed')
                AND dependency_build_plan_generation > 0
            )
        ),
    ADD CONSTRAINT derivations_dependency_build_plan_lease_state
        CHECK (
            (dependency_build_plan_status = 'calculating')
            = (dependency_build_plan_lease_expires_at IS NOT NULL)
        );
