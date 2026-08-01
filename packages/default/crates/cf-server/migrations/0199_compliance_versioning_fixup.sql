-- Migration 0199: Compliance versioning correctness fixes.
--
-- Addresses review findings:
--
-- 1. Second-order FK failures: compliance_bundle_version_policies and
--    compliance_bundle_assignments referenced version rows with ON DELETE RESTRICT.
--    A draft policy in a draft bundle cannot be deleted because its version row is
--    still referenced by the bundle membership. Similarly a draft bundle with
--    an environment assignment cannot be deleted. Both are changed to CASCADE so
--    that deleting a draft lineage propagates cleanly. Immutability for accepted
--    and deprecated rows is enforced by the trigger guards added below.
--
-- 2. Bundle CRUD sync: adds an AFTER INSERT OR UPDATE trigger on compliance_bundles
--    that mirrors the policy trigger added in 0197. The trigger also handles
--    compliance_bundle_policies changes via a separate trigger on that join table.
--
-- 3. Database-level immutability: adds BEFORE UPDATE/DELETE triggers on both
--    deployment_policy_versions and compliance_bundle_versions that reject any
--    mutation (including deletion) when the row is in the accepted or deprecated
--    state. The lineage-deletion guards in 0197 are also updated here to use the
--    same two-state immutability check (accepted OR deprecated).
--
-- 4. Bundle digest correctness: the 0197 backfill omitted framework_version and
--    description from bundle digests. This migration recomputes every backfilled
--    bundle digest using the full canonical field set. Future writes go through the
--    bundle sync trigger which includes all canonical fields.

-- ── 1. Fix second-order FK constraints ──────────────────────────────────────

-- Bundle version membership: changing RESTRICT to CASCADE so deleting a draft
-- version automatically removes its membership rows.
ALTER TABLE compliance_bundle_version_policies
    DROP CONSTRAINT IF EXISTS compliance_bundle_version_policies_policy_version_id_fkey;

ALTER TABLE compliance_bundle_version_policies
    ADD CONSTRAINT compliance_bundle_version_policies_policy_version_id_fkey
        FOREIGN KEY (policy_version_id)
        REFERENCES deployment_policy_versions(id)
        ON DELETE CASCADE;

-- Bundle assignments: changing RESTRICT to CASCADE so deleting a draft bundle
-- version removes its assignments.
ALTER TABLE compliance_bundle_assignments
    DROP CONSTRAINT IF EXISTS compliance_bundle_assignments_bundle_version_id_fkey;

ALTER TABLE compliance_bundle_assignments
    ADD CONSTRAINT compliance_bundle_assignments_bundle_version_id_fkey
        FOREIGN KEY (bundle_version_id)
        REFERENCES compliance_bundle_versions(id)
        ON DELETE CASCADE;

-- ── 2. Database-level version immutability ──────────────────────────────────

-- Reject any UPDATE or DELETE on an already-immutable policy version row.
CREATE OR REPLACE FUNCTION enforce_policy_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify or delete policy version % because it is in immutable state ''%''.',
            OLD.id, OLD.publication_state;
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_policy_version_immutable
    BEFORE UPDATE OR DELETE ON deployment_policy_versions
    FOR EACH ROW
    EXECUTE FUNCTION enforce_policy_version_immutability();

-- Reject any UPDATE or DELETE on an already-immutable bundle version row.
CREATE OR REPLACE FUNCTION enforce_bundle_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify or delete bundle version % because it is in immutable state ''%''.',
            OLD.id, OLD.publication_state;
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_bundle_version_immutable
    BEFORE UPDATE OR DELETE ON compliance_bundle_versions
    FOR EACH ROW
    EXECUTE FUNCTION enforce_bundle_version_immutability();

-- Update the lineage-deletion guards from 0197 to check both accepted AND
-- deprecated (both are immutable per the Rust model).
CREATE OR REPLACE FUNCTION prevent_delete_published_policy_lineage()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM deployment_policy_versions
        WHERE policy_id = OLD.id
          AND publication_state IN ('accepted', 'deprecated')
    ) THEN
        RAISE EXCEPTION
            'Cannot delete policy % because it has an immutable (accepted or deprecated) version.',
            OLD.id;
    END IF;
    RETURN OLD;
END;
$$;

CREATE OR REPLACE FUNCTION prevent_delete_published_bundle_lineage()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM compliance_bundle_versions
        WHERE bundle_id = OLD.id
          AND publication_state IN ('accepted', 'deprecated')
    ) THEN
        RAISE EXCEPTION
            'Cannot delete bundle % because it has an immutable (accepted or deprecated) version.',
            OLD.id;
    END IF;
    RETURN OLD;
END;
$$;

-- ── 3. Bundle CRUD sync trigger ──────────────────────────────────────────────

-- Helper: compute the canonical bundle digest from all semantic fields.
CREATE OR REPLACE FUNCTION compute_bundle_draft_digest(
    p_bundle_id uuid
) RETURNS text LANGUAGE plpgsql AS $$
DECLARE
    v_row compliance_bundles%ROWTYPE;
    v_policy_ids jsonb;
    v_digest text;
BEGIN
    SELECT * INTO v_row FROM compliance_bundles WHERE id = p_bundle_id;
    IF NOT FOUND THEN
        RETURN encode(digest('', 'sha256'), 'hex');
    END IF;

    -- Ordered policy version IDs — same ordering used by backfill and Rust.
    SELECT COALESCE(jsonb_agg(pv.id::text ORDER BY bp.policy_id), '[]'::jsonb)
    INTO v_policy_ids
    FROM compliance_bundle_policies bp
    JOIN deployment_policy_versions pv
      ON pv.policy_id = bp.policy_id
      AND pv.publication_state = 'draft'
    WHERE bp.bundle_id = p_bundle_id;

    v_digest := encode(
        digest(
            convert_to(
                jsonb_build_object(
                    'canonicalization_version', 'cf-model-json-1',
                    'description', COALESCE(v_row.description, ''),
                    'framework', v_row.framework,
                    'framework_version', COALESCE(v_row.version, ''),
                    'layer', v_row.layer,
                    'name', v_row.name,
                    'owner', v_row.owner,
                    'policy_version_ids', COALESCE(v_policy_ids, '[]'::jsonb)
                )::text,
                'UTF8'
            ),
            'sha256'
        ),
        'hex'
    );
    RETURN v_digest;
END;
$$;

-- Sync draft version on bundle INSERT or UPDATE.
CREATE OR REPLACE FUNCTION sync_bundle_draft_version()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_id uuid;
    v_digest text;
BEGIN
    v_digest := compute_bundle_draft_digest(NEW.id);

    IF TG_OP = 'INSERT' THEN
        INSERT INTO compliance_bundle_versions (
            bundle_id, version, name, framework, framework_version,
            description, layer, owner, semantic_digest
        ) VALUES (
            NEW.id, '0.1.0', NEW.name, NEW.framework, NULLIF(NEW.version, ''),
            NEW.description, NEW.layer, NEW.owner, v_digest
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
                semantic_digest = v_digest
            WHERE id = NEW.current_draft_version_id
              AND publication_state = 'draft';
        ELSE
            INSERT INTO compliance_bundle_versions (
                bundle_id, version, name, framework, framework_version,
                description, layer, owner, semantic_digest
            ) VALUES (
                NEW.id, '0.1.0', NEW.name, NEW.framework, NULLIF(NEW.version, ''),
                NEW.description, NEW.layer, NEW.owner, v_digest
            )
            RETURNING id INTO v_id;
            UPDATE compliance_bundles SET current_draft_version_id = v_id WHERE id = NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_sync_bundle_draft_version
    AFTER INSERT OR UPDATE ON compliance_bundles
    FOR EACH ROW
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION sync_bundle_draft_version();

-- Sync bundle version membership when compliance_bundle_policies changes.
CREATE OR REPLACE FUNCTION sync_bundle_version_membership()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_bundle_id uuid;
    v_version_id uuid;
    v_policy_version_id uuid;
    v_max_order integer;
    v_digest text;
BEGIN
    v_bundle_id := COALESCE(NEW.bundle_id, OLD.bundle_id);

    SELECT current_draft_version_id INTO v_version_id
    FROM compliance_bundles WHERE id = v_bundle_id;

    IF v_version_id IS NULL THEN
        RETURN COALESCE(NEW, OLD);
    END IF;

    IF TG_OP = 'INSERT' THEN
        -- Find the current draft version for this policy.
        SELECT id INTO v_policy_version_id
        FROM deployment_policy_versions
        WHERE policy_id = NEW.policy_id AND publication_state = 'draft'
        ORDER BY created_at DESC LIMIT 1;

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
        -- Remove the version membership for this policy's draft version.
        DELETE FROM compliance_bundle_version_policies
        WHERE bundle_version_id = v_version_id
          AND policy_version_id IN (
              SELECT id FROM deployment_policy_versions
              WHERE policy_id = OLD.policy_id
          );

        -- Recompact policy_order after removal.
        WITH ordered AS (
            SELECT id,
                   row_number() OVER (ORDER BY policy_order)::integer - 1 AS new_order
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = v_version_id
        )
        UPDATE compliance_bundle_version_policies bvp
        SET policy_order = ordered.new_order
        FROM ordered
        WHERE bvp.id = ordered.id;
    END IF;

    -- Recompute the bundle digest after membership change.
    v_digest := compute_bundle_draft_digest(v_bundle_id);
    UPDATE compliance_bundle_versions
    SET semantic_digest = v_digest
    WHERE id = v_version_id AND publication_state = 'draft';

    RETURN COALESCE(NEW, OLD);
END;
$$;

CREATE TRIGGER trigger_sync_bundle_version_membership
    AFTER INSERT OR DELETE ON compliance_bundle_policies
    FOR EACH ROW
    EXECUTE FUNCTION sync_bundle_version_membership();

-- ── 4. Fix backfilled bundle digests ────────────────────────────────────────
-- The 0197 migration omitted framework_version and description from the bundle
-- digest. Recompute all backfilled (version = '0.1.0') bundle version digests
-- using the same full canonical field set as the trigger function above.

UPDATE compliance_bundle_versions bv
SET semantic_digest = encode(
    digest(
        convert_to(
            jsonb_build_object(
                'canonicalization_version', 'cf-model-json-1',
                'description', COALESCE(b.description, ''),
                'framework', b.framework,
                'framework_version', COALESCE(b.version, ''),
                'layer', b.layer,
                'name', b.name,
                'owner', b.owner,
                'policy_version_ids', COALESCE(
                    (
                        SELECT jsonb_agg(pv.id::text ORDER BY bp.policy_id)
                        FROM compliance_bundle_policies bp
                        JOIN deployment_policy_versions pv
                          ON pv.policy_id = bp.policy_id
                          AND pv.version = '0.1.0'
                        WHERE bp.bundle_id = b.id
                    ),
                    '[]'::jsonb
                )
            )::text,
            'UTF8'
        ),
        'sha256'
    ),
    'hex'
)
FROM compliance_bundles b
WHERE bv.bundle_id = b.id
  AND bv.version = '0.1.0';

-- Also update the effective_set_digest in backfilled assignments to match.
UPDATE compliance_bundle_assignments a
SET effective_set_digest = bv.semantic_digest
FROM compliance_bundle_versions bv
WHERE a.bundle_version_id = bv.id;
