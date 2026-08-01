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

-- ── 12. enabled_by_default column (P1 #3) ────────────────────────────────────
-- The version model must preserve the source enabled state for interchange.
ALTER TABLE deployment_policy_versions
    ADD COLUMN IF NOT EXISTS enabled_by_default boolean;

-- Backfill from current lineage values.
UPDATE deployment_policy_versions dpv
SET enabled_by_default = dp.enabled
FROM deployment_policies dp
WHERE dpv.policy_id = dp.id
  AND dpv.enabled_by_default IS NULL;

ALTER TABLE deployment_policy_versions
    ALTER COLUMN enabled_by_default SET NOT NULL,
    ALTER COLUMN enabled_by_default SET DEFAULT true;

-- Update the policy draft sync trigger to propagate enabled state.
CREATE OR REPLACE FUNCTION sync_policy_draft_version()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_id uuid;
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO deployment_policy_versions (
            policy_id, version, name, description, policy_type,
            config, semantic_digest, enabled_by_default
        ) VALUES (
            NEW.id, '0.1.0', NEW.name, NEW.description, NEW.policy_type,
            NEW.config, 'pending', NEW.enabled
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
                semantic_digest = 'pending',
                enabled_by_default = NEW.enabled
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

-- ── 13. Reject direct insert of immutable version states (P1 #4) ──────────────
-- A version row inserted directly with 'accepted' or 'deprecated' bypasses
-- the deferred lineage-pointer validation. Require new versions to begin in a
-- mutable state.

CREATE OR REPLACE FUNCTION guard_version_insert_state()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.publication_state NOT IN ('incomplete', 'draft', 'interim') THEN
        RAISE EXCEPTION
            'New version % cannot be created in immutable state ''%''. '
            'Versions must begin in a mutable state (incomplete/draft/interim).',
            NEW.id, NEW.publication_state;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_guard_policy_version_insert_state
    BEFORE INSERT ON deployment_policy_versions
    FOR EACH ROW
    EXECUTE FUNCTION guard_version_insert_state();

CREATE TRIGGER trigger_guard_bundle_version_insert_state
    BEFORE INSERT ON compliance_bundle_versions
    FOR EACH ROW
    EXECUTE FUNCTION guard_version_insert_state();

-- ── 14. Fix sync_bundle_version_membership DELETE CTE (P1 #1) ─────────────────
-- The table has no surrogate id column, use the composite key.
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

        -- Recompact order using the composite key (table has no id column).
        WITH ordered AS (
            SELECT
                bundle_version_id,
                policy_version_id,
                (row_number() OVER (ORDER BY policy_order))::integer - 1 AS new_order
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = v_version_id
        )
        UPDATE compliance_bundle_version_policies bvp
        SET policy_order = ordered.new_order
        FROM ordered
        WHERE bvp.bundle_version_id = ordered.bundle_version_id
          AND bvp.policy_version_id = ordered.policy_version_id;
    END IF;

    UPDATE compliance_bundle_versions
    SET semantic_digest = 'pending'
    WHERE id = v_version_id
      AND publication_state IN ('incomplete', 'draft', 'interim');

    RETURN COALESCE(NEW, OLD);
END;
$$;

-- ── 15. Add enabled to trigger column list (P1 #2) ────────────────────────────
DROP TRIGGER IF EXISTS trigger_sync_policy_draft_version ON deployment_policies;
CREATE TRIGGER trigger_sync_policy_draft_version
    AFTER INSERT OR UPDATE OF name, description, policy_type, config, enabled
    ON deployment_policies
    FOR EACH ROW
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION sync_policy_draft_version();

-- ── 16. Remove selected=TRUE from bundle publication guard (P1 #4) ────────────
-- Every membership row in an accepted bundle must reference an immutable policy
-- version, regardless of the selected flag. Unselected rows are still part of
-- the bundle's portable identity and must not be mutable after publication.
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
          AND policy_version.publication_state
              NOT IN ('accepted', 'deprecated');

        IF v_count > 0 THEN
            RAISE EXCEPTION
                'Bundle version % cannot be accepted: % member policy version(s) '
                'are not in an immutable (accepted or deprecated) state. '
                'Publish the included policy versions before publishing the bundle, '
                'or remove the draft policies from the baseline.',
                NEW.id, v_count;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

-- ── 17. Fix membership recompaction: temporary offset (P1 #1) ─────────────────
-- Direct row-number compaction can violate the UNIQUE (bundle_version_id,
-- policy_order) constraint when updating multiple rows out of order. Use the
-- same +100000 offset → recompute → compact pattern as the Rust path.
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

        -- Step 1: temporarily offset all remaining rows to avoid unique-constraint
        -- collisions during recompaction.
        UPDATE compliance_bundle_version_policies
        SET policy_order = policy_order + 100000
        WHERE bundle_version_id = v_version_id;

        -- Step 2: recompute compact order from 0.
        WITH ordered AS (
            SELECT
                bundle_version_id,
                policy_version_id,
                (row_number() OVER (ORDER BY policy_order))::integer - 1 AS new_order
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = v_version_id
        )
        UPDATE compliance_bundle_version_policies bvp
        SET policy_order = ordered.new_order
        FROM ordered
        WHERE bvp.bundle_version_id = ordered.bundle_version_id
          AND bvp.policy_version_id = ordered.policy_version_id;
    END IF;

    UPDATE compliance_bundle_versions
    SET semantic_digest = 'pending'
    WHERE id = v_version_id
      AND publication_state IN ('incomplete', 'draft', 'interim');

    RETURN COALESCE(NEW, OLD);
END;
$$;

-- ── 18. Digest invalidation triggers on semantic child tables (P1 #2) ─────────
-- Mark the owning bundle version's semantic_digest as pending whenever
-- membership, exclusions, additions, or overrides change. The Rust service
-- recomputes the final value before commit; these triggers ensure startup
-- backfill can repair any direct-write or legacy-path inconsistency.

CREATE OR REPLACE FUNCTION invalidate_bundle_digest_on_membership_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE compliance_bundle_versions
    SET semantic_digest = 'pending'
    WHERE id = COALESCE(NEW.bundle_version_id, OLD.bundle_version_id)
      AND publication_state IN ('incomplete', 'draft', 'interim');
    RETURN COALESCE(NEW, OLD);
END;
$$;

DROP TRIGGER IF EXISTS trigger_invalidate_bundle_digest_membership
    ON compliance_bundle_version_policies;

CREATE TRIGGER trigger_invalidate_bundle_digest_membership
    AFTER INSERT OR UPDATE OR DELETE ON compliance_bundle_version_policies
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_bundle_digest_on_membership_change();

-- Assignment overlay children invalidate the parent assignment on change.
CREATE OR REPLACE FUNCTION invalidate_overlay_on_child_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE compliance_bundle_assignments
    SET assignment_overlay_digest = 'pending'
    WHERE id = COALESCE(NEW.assignment_id, OLD.assignment_id);
    RETURN COALESCE(NEW, OLD);
END;
$$;

CREATE TRIGGER trigger_invalidate_overlay_exclusion
    AFTER INSERT OR UPDATE OR DELETE ON compliance_assignment_exclusions
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_overlay_on_child_change();

CREATE TRIGGER trigger_invalidate_overlay_addition
    AFTER INSERT OR UPDATE OR DELETE ON compliance_assignment_additions
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_overlay_on_child_change();

CREATE TRIGGER trigger_invalidate_overlay_override
    AFTER INSERT OR UPDATE OR DELETE ON compliance_assignment_value_overrides
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_overlay_on_child_change();

-- ── 19. Source artifact integrity (P1 #3) ─────────────────────────────────────
-- Reject updates to immutable source fields after insertion. Verify that the
-- stored sha256 matches the content bytes on insert.

CREATE OR REPLACE FUNCTION guard_source_artifact_integrity()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        -- Verify that the caller-supplied digest matches the stored bytes.
        IF NEW.sha256 <> encode(digest(NEW.content, 'sha256'), 'hex') THEN
            RAISE EXCEPTION
                'Source artifact %: supplied sha256 does not match content hash.',
                NEW.id;
        END IF;
    ELSIF TG_OP = 'UPDATE' THEN
        -- Reject any change to immutable source fields.
        IF NEW.content   IS DISTINCT FROM OLD.content
        OR NEW.filename  IS DISTINCT FROM OLD.filename
        OR NEW.media_type IS DISTINCT FROM OLD.media_type
        OR NEW.sha256    IS DISTINCT FROM OLD.sha256
        OR NEW.parser_version IS DISTINCT FROM OLD.parser_version
        OR NEW.detected_xccdf_version IS DISTINCT FROM OLD.detected_xccdf_version
        OR NEW.package_context IS DISTINCT FROM OLD.package_context
        THEN
            RAISE EXCEPTION
                'Source artifact % is immutable. Only signature_details and imported_by may change.',
                OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_guard_source_artifact_integrity
    BEFORE INSERT OR UPDATE ON compliance_source_artifacts
    FOR EACH ROW
    EXECUTE FUNCTION guard_source_artifact_integrity();

-- When a source artifact is referenced by a version, prevent deletion.
CREATE OR REPLACE FUNCTION prevent_source_artifact_delete_when_referenced()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM deployment_policy_versions WHERE source_artifact_id = OLD.id
    ) OR EXISTS (
        SELECT 1 FROM compliance_bundle_versions  WHERE source_artifact_id = OLD.id
    ) THEN
        RAISE EXCEPTION
            'Cannot delete source artifact %: it is still referenced by a policy or bundle version.',
            OLD.id;
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_prevent_source_artifact_delete_when_referenced
    BEFORE DELETE ON compliance_source_artifacts
    FOR EACH ROW
    EXECUTE FUNCTION prevent_source_artifact_delete_when_referenced();

-- ── 20. Invalidate assignment overlays on membership change (P1 #1) ───────────
-- The membership trigger must also mark every assignment_overlay_digest for the
-- affected bundle version as pending, because assignment resolution reads the
-- baseline membership.  On UPDATE, if bundle_version_id changed, invalidate
-- both the old and new parent.

CREATE OR REPLACE FUNCTION invalidate_bundle_digest_on_membership_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_old_bvid uuid;
    v_new_bvid uuid;
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        v_old_bvid := OLD.bundle_version_id;
    END IF;
    IF TG_OP IN ('INSERT', 'UPDATE') THEN
        v_new_bvid := NEW.bundle_version_id;
    END IF;

    -- Invalidate bundle digest for the old parent (losing a row).
    IF v_old_bvid IS NOT NULL THEN
        UPDATE compliance_bundle_versions
        SET semantic_digest = 'pending'
        WHERE id = v_old_bvid
          AND publication_state IN ('incomplete', 'draft', 'interim');

        UPDATE compliance_bundle_assignments
        SET assignment_overlay_digest = 'pending'
        WHERE bundle_version_id = v_old_bvid;
    END IF;

    -- Invalidate bundle digest and assignments for the new parent (gaining a row
    -- or having its membership changed in-place).
    IF v_new_bvid IS NOT NULL
       AND (v_new_bvid IS DISTINCT FROM v_old_bvid OR TG_OP <> 'UPDATE')
    THEN
        UPDATE compliance_bundle_versions
        SET semantic_digest = 'pending'
        WHERE id = v_new_bvid
          AND publication_state IN ('incomplete', 'draft', 'interim');

        UPDATE compliance_bundle_assignments
        SET assignment_overlay_digest = 'pending'
        WHERE bundle_version_id = v_new_bvid;
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$;

DROP TRIGGER IF EXISTS trigger_invalidate_bundle_digest_membership
    ON compliance_bundle_version_policies;

CREATE TRIGGER trigger_invalidate_bundle_digest_membership
    AFTER INSERT OR UPDATE OR DELETE ON compliance_bundle_version_policies
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_bundle_digest_on_membership_change();

-- ── 21. Invalidate assignment overlay on enforcement_mode change (P1 #2) ──────

CREATE OR REPLACE FUNCTION invalidate_assignment_on_digest_field_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    -- enforcement_mode is the only digest-covered field on the assignment row
    -- itself that can change; all other semantics live in child tables.
    IF NEW.enforcement_mode IS DISTINCT FROM OLD.enforcement_mode THEN
        NEW.assignment_overlay_digest = 'pending';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_invalidate_assignment_on_digest_change
    BEFORE UPDATE ON compliance_bundle_assignments
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_assignment_on_digest_field_change();

-- Fix child-table overlay triggers: invalidate both old and new parent on
-- reparenting (P1 #2).

CREATE OR REPLACE FUNCTION invalidate_overlay_on_child_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_old_aid uuid;
    v_new_aid uuid;
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        v_old_aid := OLD.assignment_id;
    END IF;
    IF TG_OP IN ('INSERT', 'UPDATE') THEN
        v_new_aid := NEW.assignment_id;
    END IF;

    IF v_old_aid IS NOT NULL THEN
        UPDATE compliance_bundle_assignments
        SET assignment_overlay_digest = 'pending'
        WHERE id = v_old_aid;
    END IF;

    IF v_new_aid IS NOT NULL
       AND v_new_aid IS DISTINCT FROM v_old_aid
    THEN
        UPDATE compliance_bundle_assignments
        SET assignment_overlay_digest = 'pending'
        WHERE id = v_new_aid;
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$;

DROP TRIGGER IF EXISTS trigger_invalidate_overlay_exclusion
    ON compliance_assignment_exclusions;
DROP TRIGGER IF EXISTS trigger_invalidate_overlay_addition
    ON compliance_assignment_additions;
DROP TRIGGER IF EXISTS trigger_invalidate_overlay_override
    ON compliance_assignment_value_overrides;

CREATE TRIGGER trigger_invalidate_overlay_exclusion
    AFTER INSERT OR UPDATE OR DELETE ON compliance_assignment_exclusions
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_overlay_on_child_change();

CREATE TRIGGER trigger_invalidate_overlay_addition
    AFTER INSERT OR UPDATE OR DELETE ON compliance_assignment_additions
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_overlay_on_child_change();

CREATE TRIGGER trigger_invalidate_overlay_override
    AFTER INSERT OR UPDATE OR DELETE ON compliance_assignment_value_overrides
    FOR EACH ROW
    EXECUTE FUNCTION invalidate_overlay_on_child_change();

-- ── 22. Include import provenance in immutable artifact fields (P1 #3) ────────

CREATE OR REPLACE FUNCTION guard_source_artifact_integrity()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.sha256 <> encode(digest(NEW.content, 'sha256'), 'hex') THEN
            RAISE EXCEPTION
                'Source artifact %: supplied sha256 does not match content hash.',
                NEW.id;
        END IF;
    ELSIF TG_OP = 'UPDATE' THEN
        IF NEW.content   IS DISTINCT FROM OLD.content
        OR NEW.filename  IS DISTINCT FROM OLD.filename
        OR NEW.media_type IS DISTINCT FROM OLD.media_type
        OR NEW.sha256    IS DISTINCT FROM OLD.sha256
        OR NEW.parser_version IS DISTINCT FROM OLD.parser_version
        OR NEW.detected_xccdf_version IS DISTINCT FROM OLD.detected_xccdf_version
        OR NEW.package_context IS DISTINCT FROM OLD.package_context
        OR NEW.imported_by IS DISTINCT FROM OLD.imported_by
        OR NEW.imported_at IS DISTINCT FROM OLD.imported_at
        THEN
            RAISE EXCEPTION
                'Source artifact % is immutable. Only signature_details may change.',
                OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

-- ── 23. Protect source-object mappings from cascade deletion (P1 #4) ──────────

ALTER TABLE compliance_source_object_mappings
    DROP CONSTRAINT IF EXISTS
        compliance_source_object_mappings_source_artifact_id_fkey;

ALTER TABLE compliance_source_object_mappings
    ADD CONSTRAINT compliance_source_object_mappings_source_artifact_id_fkey
        FOREIGN KEY (source_artifact_id)
        REFERENCES compliance_source_artifacts(id)
        ON DELETE RESTRICT;

-- Include mappings in the deletion guard so every reference, direct or indirect,
-- blocks artifact removal.
CREATE OR REPLACE FUNCTION prevent_source_artifact_delete_when_referenced()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM deployment_policy_versions  WHERE source_artifact_id = OLD.id
    ) OR EXISTS (
        SELECT 1 FROM compliance_bundle_versions   WHERE source_artifact_id = OLD.id
    ) OR EXISTS (
        SELECT 1 FROM compliance_source_object_mappings WHERE source_artifact_id = OLD.id
    ) THEN
        RAISE EXCEPTION
            'Cannot delete source artifact %: it is still referenced.',
            OLD.id;
    END IF;
    RETURN OLD;
END;
$$;

-- ── 24. Invalidate digest on bundle_version_id change (P1 #1) ────────────────
-- The digest resolves baseline membership through the assignment's current
-- bundle version. Changing bundle_version_id must also mark the digest pending.

CREATE OR REPLACE FUNCTION invalidate_assignment_on_digest_field_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.enforcement_mode IS DISTINCT FROM OLD.enforcement_mode
       OR NEW.bundle_version_id IS DISTINCT FROM OLD.bundle_version_id
    THEN
        NEW.assignment_overlay_digest = 'pending';
    END IF;
    RETURN NEW;
END;
$$;

-- ── 25. Immutable source-object mappings (P1 #2) ─────────────────────────────
-- Mappings must not be deleted or modified after commit (AC #19).  Target
-- foreign keys changed from ON DELETE SET NULL to ON DELETE RESTRICT so
-- deleting a policy or bundle version while a committed mapping still
-- references it is also an error.

CREATE OR REPLACE FUNCTION guard_source_object_mapping_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION
            'Source-object mapping % is immutable and cannot be deleted.',
            OLD.id;
    END IF;

    IF (to_jsonb(NEW) - 'id') IS DISTINCT FROM (to_jsonb(OLD) - 'id') THEN
        RAISE EXCEPTION
            'Source-object mapping % is immutable and cannot be updated.',
            OLD.id;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_guard_source_object_mapping_immutability
    BEFORE UPDATE OR DELETE ON compliance_source_object_mappings
    FOR EACH ROW
    EXECUTE FUNCTION guard_source_object_mapping_immutability();

-- Change version-target FKs from SET NULL to RESTRICT so deleting a draft
-- policy or bundle that is still referenced by a committed mapping fails.
ALTER TABLE compliance_source_object_mappings
    DROP CONSTRAINT IF EXISTS
        compliance_source_object_mappings_policy_version_id_fkey;

ALTER TABLE compliance_source_object_mappings
    ADD CONSTRAINT compliance_source_object_mappings_policy_version_id_fkey
        FOREIGN KEY (policy_version_id)
        REFERENCES deployment_policy_versions(id)
        ON DELETE RESTRICT;

ALTER TABLE compliance_source_object_mappings
    DROP CONSTRAINT IF EXISTS
        compliance_source_object_mappings_bundle_version_id_fkey;

ALTER TABLE compliance_source_object_mappings
    ADD CONSTRAINT compliance_source_object_mappings_bundle_version_id_fkey
        FOREIGN KEY (bundle_version_id)
        REFERENCES compliance_bundle_versions(id)
        ON DELETE RESTRICT;

-- ── 26. Fix mapping guard to compare full row (P1) ────────────────────────────

CREATE OR REPLACE FUNCTION guard_source_object_mapping_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION
            'Source-object mapping % is immutable and cannot be deleted.',
            OLD.id;
    END IF;

    IF to_jsonb(NEW) IS DISTINCT FROM to_jsonb(OLD) THEN
        RAISE EXCEPTION
            'Source-object mapping % is immutable and cannot be updated.',
            OLD.id;
    END IF;

    RETURN NEW;
END;
$$;

-- ── 27. Fix source-artifact guard to use full-row comparison (P1) ────────────

CREATE OR REPLACE FUNCTION guard_source_artifact_integrity()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.sha256 <> encode(digest(NEW.content, 'sha256'), 'hex') THEN
            RAISE EXCEPTION
                'Source artifact %: supplied sha256 does not match content hash.',
                NEW.id;
        END IF;
    ELSIF TG_OP = 'UPDATE' THEN
        IF (to_jsonb(NEW) - 'signature_details')
           IS DISTINCT FROM (to_jsonb(OLD) - 'signature_details')
        THEN
            RAISE EXCEPTION
                'Source artifact % is immutable. Only signature_details may change.',
                OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$;
