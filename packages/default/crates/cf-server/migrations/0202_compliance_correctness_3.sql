-- Migration 0202: Compliance foundation correctness – round 7.
--
-- 1. Pointer integrity constraints (P1 #3):
--    The current_draft_version_id and current_published_version_id columns are
--    unconstrained foreign keys. Add CHECK triggers that verify:
--      a) the referenced version's policy_id / bundle_id matches the lineage;
--      b) current_draft_version_id references a mutable (draft/incomplete/interim)
--         version;
--      c) current_published_version_id references an immutable (accepted/deprecated)
--         version.
--    Also make the membership trigger raise an error instead of silently skipping
--    when no valid exact policy version exists.
--
-- 2. Remove the implicit `0.1.0` fallback version name from triggers (P1 #2
--    partial): when both pointers are null the trigger now raises an error instead
--    of inserting a second `0.1.0` row that would violate the unique constraint on
--    (policy_id/bundle_id, version). Derived-draft creation is implemented in
--    Rust (see compliance.rs) as an explicit transactional operation.

-- ── 1a. Policy pointer integrity ─────────────────────────────────────────────

CREATE OR REPLACE FUNCTION enforce_policy_version_pointer_integrity()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.current_draft_version_id IS NOT NULL THEN
        -- Must belong to this policy lineage.
        IF NOT EXISTS (
            SELECT 1 FROM deployment_policy_versions
            WHERE id = NEW.current_draft_version_id
              AND policy_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'current_draft_version_id % does not belong to policy %',
                NEW.current_draft_version_id, NEW.id;
        END IF;
        -- Must be a mutable state.
        IF NOT EXISTS (
            SELECT 1 FROM deployment_policy_versions
            WHERE id = NEW.current_draft_version_id
              AND publication_state IN ('incomplete', 'draft', 'interim')
        ) THEN
            RAISE EXCEPTION
                'current_draft_version_id % is not in a mutable publication state',
                NEW.current_draft_version_id;
        END IF;
    END IF;

    IF NEW.current_published_version_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM deployment_policy_versions
            WHERE id = NEW.current_published_version_id
              AND policy_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'current_published_version_id % does not belong to policy %',
                NEW.current_published_version_id, NEW.id;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM deployment_policy_versions
            WHERE id = NEW.current_published_version_id
              AND publication_state IN ('accepted', 'deprecated')
        ) THEN
            RAISE EXCEPTION
                'current_published_version_id % is not in an immutable publication state',
                NEW.current_published_version_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_policy_version_pointer_integrity
    BEFORE INSERT OR UPDATE OF current_draft_version_id, current_published_version_id
    ON deployment_policies
    FOR EACH ROW
    WHEN (
        NEW.current_draft_version_id IS DISTINCT FROM OLD.current_draft_version_id
     OR NEW.current_published_version_id IS DISTINCT FROM OLD.current_published_version_id
     OR TG_OP = 'INSERT'
    )
    EXECUTE FUNCTION enforce_policy_version_pointer_integrity();

-- ── 1b. Bundle pointer integrity ──────────────────────────────────────────────

CREATE OR REPLACE FUNCTION enforce_bundle_version_pointer_integrity()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.current_draft_version_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM compliance_bundle_versions
            WHERE id = NEW.current_draft_version_id
              AND bundle_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'current_draft_version_id % does not belong to bundle %',
                NEW.current_draft_version_id, NEW.id;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM compliance_bundle_versions
            WHERE id = NEW.current_draft_version_id
              AND publication_state IN ('incomplete', 'draft', 'interim')
        ) THEN
            RAISE EXCEPTION
                'current_draft_version_id % is not in a mutable publication state',
                NEW.current_draft_version_id;
        END IF;
    END IF;

    IF NEW.current_published_version_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM compliance_bundle_versions
            WHERE id = NEW.current_published_version_id
              AND bundle_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'current_published_version_id % does not belong to bundle %',
                NEW.current_published_version_id, NEW.id;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM compliance_bundle_versions
            WHERE id = NEW.current_published_version_id
              AND publication_state IN ('accepted', 'deprecated')
        ) THEN
            RAISE EXCEPTION
                'current_published_version_id % is not in an immutable publication state',
                NEW.current_published_version_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_bundle_version_pointer_integrity
    BEFORE INSERT OR UPDATE OF current_draft_version_id, current_published_version_id
    ON compliance_bundles
    FOR EACH ROW
    WHEN (
        NEW.current_draft_version_id IS DISTINCT FROM OLD.current_draft_version_id
     OR NEW.current_published_version_id IS DISTINCT FROM OLD.current_published_version_id
     OR TG_OP = 'INSERT'
    )
    EXECUTE FUNCTION enforce_bundle_version_pointer_integrity();

-- ── 1c. Membership trigger: error on missing version, not silent skip ─────────

CREATE OR REPLACE FUNCTION sync_bundle_version_membership()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_bundle_id       uuid;
    v_version_id      uuid;
    v_policy_version_id uuid;
    v_max_order       integer;
BEGIN
    v_bundle_id := COALESCE(NEW.bundle_id, OLD.bundle_id);

    SELECT current_draft_version_id INTO v_version_id
    FROM compliance_bundles WHERE id = v_bundle_id;

    IF v_version_id IS NULL THEN
        -- No current draft version: this should not happen after migrations but
        -- guard defensively.
        IF TG_OP = 'INSERT' THEN
            RAISE EXCEPTION
                'Cannot add policy to bundle %: bundle has no current draft version.',
                v_bundle_id;
        END IF;
        RETURN OLD;
    END IF;

    IF TG_OP = 'INSERT' THEN
        -- Require an exact version pointer on the policy lineage (P1 #3).
        SELECT COALESCE(dp.current_draft_version_id, dp.current_published_version_id)
        INTO v_policy_version_id
        FROM deployment_policies dp
        WHERE dp.id = NEW.policy_id;

        IF v_policy_version_id IS NULL THEN
            RAISE EXCEPTION
                'Cannot add policy % to bundle %: policy has no versioned draft or published version.',
                NEW.policy_id, v_bundle_id;
        END IF;

        -- Verify cross-lineage integrity: the resolved version must belong to
        -- the policy we are adding.
        IF NOT EXISTS (
            SELECT 1 FROM deployment_policy_versions
            WHERE id = v_policy_version_id AND policy_id = NEW.policy_id
        ) THEN
            RAISE EXCEPTION
                'Pointer integrity violation: policy version % does not belong to policy %.',
                v_policy_version_id, NEW.policy_id;
        END IF;

        SELECT COALESCE(MAX(policy_order), -1) + 1 INTO v_max_order
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = v_version_id;

        INSERT INTO compliance_bundle_version_policies
            (bundle_version_id, policy_version_id, policy_order)
        VALUES (v_version_id, v_policy_version_id, v_max_order)
        ON CONFLICT DO NOTHING;

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
    WHERE id = v_version_id AND publication_state IN ('incomplete', 'draft', 'interim');

    RETURN COALESCE(NEW, OLD);
END;
$$;

-- ── 2. Remove implicit 0.1.0 fallback from sync_policy_draft_version ─────────
-- When the pointer is null during an UPDATE the trigger now raises rather than
-- silently inserting a conflicting version identity. Derived-draft creation is
-- an explicit Rust operation.

CREATE OR REPLACE FUNCTION sync_policy_draft_version()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_id uuid;
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO deployment_policy_versions (
            policy_id, version, name, description, policy_type, config, semantic_digest
        ) VALUES (
            NEW.id, '0.1.0', NEW.name, NEW.description, NEW.policy_type, NEW.config,
            'pending'
        )
        RETURNING id INTO v_id;
        UPDATE deployment_policies SET current_draft_version_id = v_id WHERE id = NEW.id;

    ELSIF TG_OP = 'UPDATE' THEN
        IF NEW.current_draft_version_id IS NOT NULL THEN
            UPDATE deployment_policy_versions
            SET name = NEW.name,
                description = NEW.description,
                policy_type = NEW.policy_type,
                config = NEW.config,
                semantic_digest = 'pending'
            WHERE id = NEW.current_draft_version_id
              AND publication_state IN ('incomplete', 'draft', 'interim');
        ELSE
            -- No draft pointer: caller must create a derived draft first.
            RAISE EXCEPTION
                'Cannot update policy %: no mutable draft version exists. '
                'Create a derived draft before editing.',
                NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION sync_bundle_draft_version()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_id uuid;
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO compliance_bundle_versions (
            bundle_id, version, name, framework, framework_version,
            description, layer, owner, semantic_digest
        ) VALUES (
            NEW.id, '0.1.0', NEW.name, NEW.framework, NULLIF(NEW.version, ''),
            NEW.description, NEW.layer, NEW.owner, 'pending'
        )
        RETURNING id INTO v_id;
        UPDATE compliance_bundles SET current_draft_version_id = v_id WHERE id = NEW.id;

    ELSIF TG_OP = 'UPDATE' THEN
        IF NEW.current_draft_version_id IS NOT NULL THEN
            UPDATE compliance_bundle_versions
            SET name = NEW.name,
                framework = NEW.framework,
                framework_version = NULLIF(NEW.version, ''),
                description = NEW.description,
                layer = NEW.layer,
                owner = NEW.owner,
                semantic_digest = 'pending'
            WHERE id = NEW.current_draft_version_id
              AND publication_state IN ('incomplete', 'draft', 'interim');
        ELSE
            RAISE EXCEPTION
                'Cannot update bundle %: no mutable draft version exists. '
                'Create a derived draft before editing.',
                NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;
