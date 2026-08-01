-- Migration 0202: Compliance foundation correctness (squash of former 0202+0203).
--
-- 1. Pointer-integrity constraints on deployment_policies and compliance_bundles.
-- 2. Deferred state-change validation with accepted-pointer enforcement.
-- 3. Immutability guard on compliance_bundle_version_policies (checks both OLD/NEW).
-- 4. Accepted→deprecated transition: permitted explicitly; all other semantic
--    mutations to accepted rows remain rejected.
-- 5. Policy draft sync trigger: uses mutable states, errors on missing draft,
--    never creates an implicit derived draft.
-- 6. Bundle membership and draft sync triggers: error-on-missing semantics.
-- 7. Rebuilt assignment updated_at trigger.
-- 8. Corrected accepted→deprecated comparison (P1 #3): use full-row minus
--    publication_state to reject every non-lifecycle change.
-- 9. Bundle publication guard (P1 #2): deferred validation that every selected
--    membership references an immutable (accepted/deprecated) policy version.
-- 10. Restricted sync triggers (P1 #1): the lineage sync triggers fire only
--    for semantic-column updates so pointer-only updates (publication) do not
--    provoke a "no draft version" error.

-- ── 1. Policy pointer-integrity triggers ─────────────────────────────────────

CREATE OR REPLACE FUNCTION enforce_policy_version_pointer_integrity()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.current_draft_version_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM deployment_policy_versions
            WHERE id = NEW.current_draft_version_id
              AND policy_id = NEW.id
        ) THEN
            RAISE EXCEPTION
                'current_draft_version_id % does not belong to policy %',
                NEW.current_draft_version_id, NEW.id;
        END IF;
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

-- Separate INSERT trigger (no WHEN, cannot reference OLD).
CREATE TRIGGER trigger_policy_version_pointer_integrity_insert
    BEFORE INSERT ON deployment_policies
    FOR EACH ROW
    EXECUTE FUNCTION enforce_policy_version_pointer_integrity();

-- Separate UPDATE trigger (WHEN references OLD/NEW only, no TG_OP).
CREATE TRIGGER trigger_policy_version_pointer_integrity_update
    BEFORE UPDATE OF current_draft_version_id, current_published_version_id
    ON deployment_policies
    FOR EACH ROW
    WHEN (
        OLD.current_draft_version_id IS DISTINCT FROM NEW.current_draft_version_id
     OR OLD.current_published_version_id IS DISTINCT FROM NEW.current_published_version_id
    )
    EXECUTE FUNCTION enforce_policy_version_pointer_integrity();

-- ── 2. Bundle pointer-integrity triggers ──────────────────────────────────────

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

CREATE TRIGGER trigger_bundle_version_pointer_integrity_insert
    BEFORE INSERT ON compliance_bundles
    FOR EACH ROW
    EXECUTE FUNCTION enforce_bundle_version_pointer_integrity();

CREATE TRIGGER trigger_bundle_version_pointer_integrity_update
    BEFORE UPDATE OF current_draft_version_id, current_published_version_id
    ON compliance_bundles
    FOR EACH ROW
    WHEN (
        OLD.current_draft_version_id IS DISTINCT FROM NEW.current_draft_version_id
     OR OLD.current_published_version_id IS DISTINCT FROM NEW.current_published_version_id
    )
    EXECUTE FUNCTION enforce_bundle_version_pointer_integrity();

-- ── 3. Deferred state-change validation ───────────────────────────────────────
-- Fires at COMMIT so both the version state and the lineage pointer can be
-- updated in any order within the same transaction.

CREATE OR REPLACE FUNCTION validate_policy_lineage_pointer_after_state_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_draft_id     uuid;
    v_published_id uuid;
BEGIN
    SELECT current_draft_version_id, current_published_version_id
    INTO v_draft_id, v_published_id
    FROM deployment_policies WHERE id = NEW.policy_id;

    -- After moving to accepted: must be the lineage's current_published pointer.
    IF NEW.publication_state = 'accepted' THEN
        IF v_published_id IS DISTINCT FROM NEW.id THEN
            RAISE EXCEPTION
                'Policy version % became accepted but current_published_version_id on '
                'policy % is %. Update the pointer in the same transaction.',
                NEW.id, NEW.policy_id, v_published_id;
        END IF;
        IF v_draft_id = NEW.id THEN
            RAISE EXCEPTION
                'Policy version % became accepted but is still current_draft_version_id '
                'on policy %. Clear the draft pointer.',
                NEW.id, NEW.policy_id;
        END IF;
    END IF;

    -- Any immutable state: must not remain draft pointer.
    IF NEW.publication_state IN ('accepted', 'deprecated') THEN
        IF v_draft_id = NEW.id THEN
            RAISE EXCEPTION
                'Policy version % moved to immutable state ''%'' but is still '
                'current_draft_version_id on policy %.',
                NEW.id, NEW.publication_state, NEW.policy_id;
        END IF;
    END IF;

    -- Mutable states: must not remain published pointer.
    IF NEW.publication_state IN ('incomplete', 'draft', 'interim') THEN
        IF v_published_id = NEW.id THEN
            RAISE EXCEPTION
                'Policy version % is in mutable state ''%'' but is still '
                'current_published_version_id on policy %.',
                NEW.id, NEW.publication_state, NEW.policy_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_validate_policy_lineage_on_state_change
    AFTER UPDATE OF publication_state ON deployment_policy_versions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (OLD.publication_state IS DISTINCT FROM NEW.publication_state)
    EXECUTE FUNCTION validate_policy_lineage_pointer_after_state_change();

CREATE OR REPLACE FUNCTION validate_bundle_lineage_pointer_after_state_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_draft_id     uuid;
    v_published_id uuid;
BEGIN
    SELECT current_draft_version_id, current_published_version_id
    INTO v_draft_id, v_published_id
    FROM compliance_bundles WHERE id = NEW.bundle_id;

    IF NEW.publication_state = 'accepted' THEN
        IF v_published_id IS DISTINCT FROM NEW.id THEN
            RAISE EXCEPTION
                'Bundle version % became accepted but current_published_version_id on '
                'bundle % is %. Update the pointer in the same transaction.',
                NEW.id, NEW.bundle_id, v_published_id;
        END IF;
        IF v_draft_id = NEW.id THEN
            RAISE EXCEPTION
                'Bundle version % became accepted but is still current_draft_version_id '
                'on bundle %. Clear the draft pointer.',
                NEW.id, NEW.bundle_id;
        END IF;
    END IF;

    IF NEW.publication_state IN ('accepted', 'deprecated') THEN
        IF v_draft_id = NEW.id THEN
            RAISE EXCEPTION
                'Bundle version % moved to immutable state ''%'' but is still '
                'current_draft_version_id on bundle %.',
                NEW.id, NEW.publication_state, NEW.bundle_id;
        END IF;
    END IF;

    IF NEW.publication_state IN ('incomplete', 'draft', 'interim') THEN
        IF v_published_id = NEW.id THEN
            RAISE EXCEPTION
                'Bundle version % is in mutable state ''%'' but is still '
                'current_published_version_id on bundle %.',
                NEW.id, NEW.publication_state, NEW.bundle_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_validate_bundle_lineage_on_state_change
    AFTER UPDATE OF publication_state ON compliance_bundle_versions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (OLD.publication_state IS DISTINCT FROM NEW.publication_state)
    EXECUTE FUNCTION validate_bundle_lineage_pointer_after_state_change();

-- ── 4. Immutability guard on bundle version membership (P1 #3) ────────────────
-- Prevents any change to compliance_bundle_version_policies when the parent
-- bundle version is in an immutable state.

CREATE OR REPLACE FUNCTION guard_bundle_version_membership_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
    v_state      text;
BEGIN
    v_version_id := COALESCE(NEW.bundle_version_id, OLD.bundle_version_id);
    SELECT publication_state INTO v_state
    FROM compliance_bundle_versions WHERE id = v_version_id;

    IF v_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify membership of bundle version % because it is in '
            'immutable state ''%''.',
            v_version_id, v_state;
    END IF;
    RETURN COALESCE(NEW, OLD);
END;
$$;

CREATE TRIGGER trigger_guard_bundle_version_membership_immutability
    BEFORE INSERT OR UPDATE OR DELETE ON compliance_bundle_version_policies
    FOR EACH ROW
    EXECUTE FUNCTION guard_bundle_version_membership_immutability();

-- ── 5. Rebuild assignment updated_at trigger (column renamed in 0201) ─────────

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

-- ── 6. Correct bundle membership and draft sync triggers ──────────────────────

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
        IF TG_OP = 'INSERT' THEN
            RAISE EXCEPTION
                'Cannot add policy to bundle %: bundle has no current draft version.',
                v_bundle_id;
        END IF;
        RETURN OLD;
    END IF;

    IF TG_OP = 'INSERT' THEN
        SELECT COALESCE(dp.current_draft_version_id, dp.current_published_version_id)
        INTO v_policy_version_id
        FROM deployment_policies dp
        WHERE dp.id = NEW.policy_id;

        IF v_policy_version_id IS NULL THEN
            RAISE EXCEPTION
                'Cannot add policy % to bundle %: policy has no versioned draft or '
                'published version.',
                NEW.policy_id, v_bundle_id;
        END IF;

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

    UPDATE compliance_bundle_versions
    SET semantic_digest = 'pending'
    WHERE id = v_version_id
      AND publication_state IN ('incomplete', 'draft', 'interim');

    RETURN COALESCE(NEW, OLD);
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

-- Update env-assignment insert trigger to use the renamed column.
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

-- ── 7. Fix membership immutability guard: check both OLD and NEW ─────────────
-- P1 #2: During UPDATE, must reject when EITHER old (published row being taken
-- away) OR new parent is immutable.
CREATE OR REPLACE FUNCTION guard_bundle_version_membership_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_state text;
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        SELECT publication_state INTO v_state
        FROM compliance_bundle_versions WHERE id = OLD.bundle_version_id;
        IF v_state IN ('accepted', 'deprecated') THEN
            RAISE EXCEPTION
                'Cannot remove membership from immutable bundle version %.',
                OLD.bundle_version_id;
        END IF;
    END IF;

    IF TG_OP IN ('INSERT', 'UPDATE') THEN
        SELECT publication_state INTO v_state
        FROM compliance_bundle_versions WHERE id = NEW.bundle_version_id;
        IF v_state IN ('accepted', 'deprecated') THEN
            RAISE EXCEPTION
                'Cannot add membership to immutable bundle version %.',
                NEW.bundle_version_id;
        END IF;
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$;

-- ── 8. Permit accepted → deprecated transition ──────────────────────────────
-- Replace the blanket-reject triggers from 0199 with versions that allow the
-- single lifecycle transition while rejecting all semantic field changes.
CREATE OR REPLACE FUNCTION enforce_bundle_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state = 'accepted' THEN
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete accepted bundle version %.', OLD.id;
        END IF;
        IF NEW.publication_state = 'deprecated' THEN
            -- P1 #3: Compare every non-lifecycle column. Only publication_state may change.
            IF (to_jsonb(NEW) - 'publication_state')
               IS DISTINCT FROM (to_jsonb(OLD) - 'publication_state')
            THEN
                RAISE EXCEPTION
                    'Deprecating accepted bundle version % but non-lifecycle fields changed.', OLD.id;
            END IF;
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'Cannot modify accepted bundle version %.', OLD.id;
    END IF;
    IF OLD.publication_state = 'deprecated' THEN
        RAISE EXCEPTION 'Cannot modify deprecated bundle version %.', OLD.id;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_policy_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state = 'accepted' THEN
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'Cannot delete accepted policy version %.', OLD.id;
        END IF;
        IF NEW.publication_state = 'deprecated' THEN
            IF (to_jsonb(NEW) - 'publication_state')
               IS DISTINCT FROM (to_jsonb(OLD) - 'publication_state')
            THEN
                RAISE EXCEPTION
                    'Deprecating accepted policy version % but non-lifecycle fields changed.', OLD.id;
            END IF;
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'Cannot modify accepted policy version %.', OLD.id;
    END IF;
    IF OLD.publication_state = 'deprecated' THEN
        RAISE EXCEPTION 'Cannot modify deprecated policy version %.', OLD.id;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

-- ── 9. Fix policy draft sync trigger ─────────────────────────────────────────
-- The active version from 0200 still uses 'draft' as the only mutable state.
-- This replacement: 1) updates for all mutable states; 2) errors when no draft
-- exists; 3) never creates an implicit derived draft.
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
            RAISE EXCEPTION
                'Cannot update policy %: no mutable draft version exists. '
                'Create a derived draft before editing.',
                NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

-- ── 10. Restrict sync triggers to semantic columns only (P1 #1) ───────────────
-- The original trigger definitions fired on ANY update to the lineage, including
-- pointer-only updates during publication.  Drop and recreate with explicit
-- column filters so clearing current_draft_version_id does not trigger the
-- "no draft version" error during publication.

DROP TRIGGER IF EXISTS trigger_sync_policy_draft_version ON deployment_policies;
CREATE TRIGGER trigger_sync_policy_draft_version
    AFTER INSERT OR UPDATE OF name, description, policy_type, config
    ON deployment_policies
    FOR EACH ROW
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION sync_policy_draft_version();

DROP TRIGGER IF EXISTS trigger_sync_bundle_draft_version ON compliance_bundles;
CREATE TRIGGER trigger_sync_bundle_draft_version
    AFTER INSERT OR UPDATE OF name, framework, version, description
    ON compliance_bundles
    FOR EACH ROW
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION sync_bundle_draft_version();

-- ── 11. Deferred bundle publication guard (P1 #2) ─────────────────────────────
-- Before a bundle version can become accepted, every selected membership row
-- must reference an immutable (accepted/deprecated) policy version.
-- This fires at COMMIT time so the publication service can atomically publish
-- included policy drafts in the same transaction.

CREATE OR REPLACE FUNCTION validate_bundle_policy_versions_on_accept()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_count bigint;
BEGIN
    IF NEW.publication_state = 'accepted' THEN
        SELECT COUNT(*) INTO v_count
        FROM compliance_bundle_version_policies membership
        JOIN deployment_policy_versions policy_version
          ON policy_version.id = membership.policy_version_id
        WHERE membership.bundle_version_id = NEW.id
          AND membership.selected = TRUE
          AND policy_version.publication_state
              NOT IN ('accepted', 'deprecated');

        IF v_count > 0 THEN
            RAISE EXCEPTION
                'Bundle version % cannot be accepted: % selected member policy version(s) '
                'are not in an immutable (accepted or deprecated) state. '
                'Publish the included policy versions before publishing the bundle, '
                'or remove the draft policies from the baseline.',
                NEW.id, v_count;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_validate_bundle_policy_versions_on_accept
    AFTER UPDATE OF publication_state ON compliance_bundle_versions
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (OLD.publication_state IS DISTINCT FROM NEW.publication_state)
    EXECUTE FUNCTION validate_bundle_policy_versions_on_accept();
