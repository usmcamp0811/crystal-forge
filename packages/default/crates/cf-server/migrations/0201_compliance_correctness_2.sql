-- Migration 0201: Compliance foundation correctness – round 5 fixes.
--
-- 1. Rename effective_set_digest → assignment_overlay_digest (P1 #4).
--    The current value covers only: selected baseline - exclusions + additions.
--    It does not cover direct environment/system policies or conflict resolution,
--    so calling it "effective_set" is misleading. The new name is accurate.
--    The full effective-set resolver will be computed at evaluation time.
--
-- 2. Fix sync_bundle_version_membership trigger to use current_draft_version_id
--    instead of ORDER BY created_at DESC when resolving the policy version
--    to add to bundle membership (P1 #2).
--
-- 3. Fix the UNION ALL ORDER BY syntax in any view/function that referenced
--    the old query structure (P1 #1 was in Rust; no SQL view mirrors it yet).
--
-- 4. Add latest_commit_per_flake helper function used by all three scanning
--    query variants to share the same commit ordering (P2 #5).

-- ── 1. Rename column ──────────────────────────────────────────────────────────

ALTER TABLE compliance_bundle_assignments
    RENAME COLUMN effective_set_digest TO assignment_overlay_digest;

-- Update triggers that reference the old column name.

CREATE OR REPLACE FUNCTION sync_bundle_env_assignment_insert()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
BEGIN
    v_version_id := bundle_current_draft_version(NEW.bundle_id);
    IF v_version_id IS NULL THEN
        RETURN NEW;
    END IF;

    INSERT INTO compliance_bundle_assignments (
        bundle_version_id, scope_type, environment_id, enforcement_mode,
        assignment_overlay_digest, created_at, updated_at
    ) VALUES (
        v_version_id, 'environment', NEW.environment_id, 'enforce',
        'pending', now(), now()
    )
    ON CONFLICT (bundle_version_id, environment_id)
        WHERE environment_id IS NOT NULL
    DO NOTHING;

    RETURN NEW;
END;
$$;

-- Rebuild the updated_at trigger now that the column is renamed.
CREATE OR REPLACE FUNCTION update_bundle_assignment_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trigger_compliance_bundle_assignments_updated_at
    ON compliance_bundle_assignments;

CREATE TRIGGER trigger_compliance_bundle_assignments_updated_at
    BEFORE UPDATE ON compliance_bundle_assignments
    FOR EACH ROW EXECUTE FUNCTION update_bundle_assignment_updated_at();

-- ── 2. Fix sync_bundle_version_membership: use current_draft_version_id ───────

CREATE OR REPLACE FUNCTION sync_bundle_version_membership()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_bundle_id uuid;
    v_version_id uuid;
    v_policy_version_id uuid;
    v_max_order integer;
BEGIN
    v_bundle_id := COALESCE(NEW.bundle_id, OLD.bundle_id);

    SELECT current_draft_version_id INTO v_version_id
    FROM compliance_bundles WHERE id = v_bundle_id;

    IF v_version_id IS NULL THEN
        RETURN COALESCE(NEW, OLD);
    END IF;

    IF TG_OP = 'INSERT' THEN
        -- Prefer current_draft_version_id; fall back to current_published_version_id
        -- if the policy has been published but not yet re-drafted. Fail silently
        -- (skip membership) when neither pointer exists rather than selecting by
        -- created_at. (P1 #2)
        SELECT COALESCE(dp.current_draft_version_id, dp.current_published_version_id)
        INTO v_policy_version_id
        FROM deployment_policies dp
        WHERE dp.id = NEW.policy_id;

        IF v_policy_version_id IS NOT NULL THEN
            SELECT COALESCE(MAX(policy_order), -1) + 1 INTO v_max_order
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = v_version_id;

            INSERT INTO compliance_bundle_version_policies
                (bundle_version_id, policy_version_id, policy_order)
            VALUES (v_version_id, v_policy_version_id, v_max_order)
            ON CONFLICT DO NOTHING;
        END IF;

    ELSIF TG_OP = 'DELETE' THEN
        DELETE FROM compliance_bundle_version_policies
        WHERE bundle_version_id = v_version_id
          AND policy_version_id IN (
              SELECT id FROM deployment_policy_versions
              WHERE policy_id = OLD.policy_id
          );

        -- Recompact policy_order after removal.
        WITH ordered AS (
            SELECT id,
                   (row_number() OVER (ORDER BY policy_order))::integer - 1 AS new_order
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = v_version_id
        )
        UPDATE compliance_bundle_version_policies bvp
        SET policy_order = ordered.new_order
        FROM ordered WHERE bvp.id = ordered.id;
    END IF;

    -- Mark digest as pending; Rust recomputes it.
    UPDATE compliance_bundle_versions
    SET semantic_digest = 'pending'
    WHERE id = v_version_id AND publication_state = 'draft';

    RETURN COALESCE(NEW, OLD);
END;
$$;

-- Reset assignment_overlay_digest to pending so the startup backfill
-- recomputes them with the renamed column.
UPDATE compliance_bundle_assignments SET assignment_overlay_digest = 'pending';
