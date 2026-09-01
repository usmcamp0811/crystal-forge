-- COMPATIBILITY: Release A retains derivations_derivation_path_unique because
-- an older server can remain active and execute ON CONFLICT (derivation_path).
-- TASK-443 removes this constraint only after release-A processes are drained.

-- Historical attempts predate evaluation-wide dependency planning. Keep their
-- existing jobs available during a rolling upgrade through an explicit state.
ALTER TABLE evaluation_attempts
    ADD COLUMN dependency_plan_barrier text NOT NULL DEFAULT 'legacy_released'
        CHECK (dependency_plan_barrier IN ('legacy_released', 'planning', 'ready', 'cancelled')),
    ADD COLUMN dependency_plan_expected_count integer NOT NULL DEFAULT 0
        CHECK (dependency_plan_expected_count >= 0),
    ADD COLUMN dependency_plan_terminal_count integer NOT NULL DEFAULT 0
        CHECK (
            dependency_plan_terminal_count >= 0
            AND dependency_plan_terminal_count <= dependency_plan_expected_count
        ),
    ADD CONSTRAINT evaluation_attempts_dependency_plan_release_counts CHECK (
        dependency_plan_barrier <> 'ready'
        OR dependency_plan_terminal_count = dependency_plan_expected_count
    );

-- New evaluation writers tag every graph-relevant derivation with the immutable
-- attempt identity. A nullable tag is required for historical and commitless rows.
ALTER TABLE derivations
    ADD COLUMN evaluation_attempt_id uuid REFERENCES evaluation_attempts(id) ON DELETE RESTRICT;

CREATE INDEX derivations_evaluation_attempt_id_idx
    ON derivations (evaluation_attempt_id)
    WHERE evaluation_attempt_id IS NOT NULL;

-- A commit-backed derivation is releasable only for the commit's current
-- completed attempt. legacy_released is the rolling-upgrade compatibility path;
-- strict ready attempts additionally require an exact derivation attempt tag.
CREATE FUNCTION derivation_evaluation_barrier_released(p_derivation_id integer)
RETURNS boolean AS $$
    SELECT d.commit_id IS NULL OR EXISTS (
        SELECT 1
        FROM derivations guarded_d
        JOIN commits c ON c.id = guarded_d.commit_id
        JOIN evaluation_attempts ea
          ON ea.commit_id = c.id
         AND ea.attempt_number = c.evaluation_attempt_count
        WHERE guarded_d.id = p_derivation_id
          AND c.evaluation_status = 'complete'
          AND ea.status = 'complete'
          AND (
              ea.dependency_plan_barrier = 'legacy_released'
              OR (
                  ea.dependency_plan_barrier = 'ready'
                  AND guarded_d.evaluation_attempt_id = ea.id
              )
          )
    )
    FROM derivations d
    WHERE d.id = p_derivation_id;
$$ LANGUAGE sql STABLE;

-- SECURITY: Building always requires a completed released attempt. A queued
-- write also permits the current legacy_released attempt while an old server
-- is still evaluating during a rolling upgrade. Claim queries remain strict,
-- so the queued row cannot become building until that legacy attempt completes.
CREATE FUNCTION enforce_build_job_evaluation_barrier() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.derivation_id IS DISTINCT FROM OLD.derivation_id THEN
        RAISE EXCEPTION 'build job derivation identity is immutable'
            USING ERRCODE = '23514';
    END IF;

    IF NEW.status NOT IN ('queued', 'building') THEN
        RETURN NEW;
    END IF;

    -- CONCURRENCY: Do not lock the commit row after the build-job statement
    -- owns its row. Reset locks the commit before it inspects build jobs, so a
    -- commit lock here would invert that order. This statement snapshot accepts
    -- only a committed release; a ready attempt does not return to planning.
    IF derivation_evaluation_barrier_released(NEW.derivation_id) THEN
        RETURN NEW;
    END IF;

    IF NEW.status = 'queued' AND EXISTS (
        SELECT 1
        FROM derivations d
        JOIN commits c ON c.id = d.commit_id
        JOIN evaluation_attempts ea
          ON ea.commit_id = c.id
         AND ea.attempt_number = c.evaluation_attempt_count
        WHERE d.id = NEW.derivation_id
          AND c.evaluation_status IN ('in_progress', 'complete')
          AND ea.status IN ('in_progress', 'complete')
          AND ea.dependency_plan_barrier = 'legacy_released'
    ) THEN
        RETURN NEW;
    END IF;

    IF NEW.status IN ('queued', 'building') THEN
        RAISE EXCEPTION 'evaluation dependency-plan barrier is not released for derivation %',
            NEW.derivation_id USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER enforce_build_job_evaluation_barrier
BEFORE INSERT OR UPDATE OF status, derivation_id ON build_jobs
FOR EACH ROW EXECUTE FUNCTION enforce_build_job_evaluation_barrier();

-- Legacy workers reserve derivations directly instead of transitioning a
-- build_jobs row. Apply the same release predicate and immutable ownership.
CREATE FUNCTION enforce_build_reservation_evaluation_barrier() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.derivation_id IS DISTINCT FROM OLD.derivation_id THEN
        RAISE EXCEPTION 'build reservation derivation identity is immutable'
            USING ERRCODE = '23514';
    END IF;

    -- CONCURRENCY: Use the same committed statement-snapshot predicate as the
    -- build-job trigger. Taking a commit lock after the reservation row is
    -- owned would invert reset's commit-then-reservation order.
    IF NOT derivation_evaluation_barrier_released(NEW.derivation_id) THEN
        RAISE EXCEPTION 'evaluation dependency-plan barrier is not released for reservation derivation %',
            NEW.derivation_id USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER enforce_build_reservation_evaluation_barrier
BEFORE INSERT OR UPDATE OF derivation_id ON build_reservations
FOR EACH ROW EXECUTE FUNCTION enforce_build_reservation_evaluation_barrier();

-- COMPATIBILITY: Keep the default legacy_released. An old server can create an
-- attempt after this migration during a rolling upgrade. New servers explicitly
-- set planning when they claim an attempt.
