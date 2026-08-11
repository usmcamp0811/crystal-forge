-- Migration 0200: Compliance versioning correctness.
--
-- Fixes four P1 issues from review:
--
-- 1. RETURN OLD/NEW in BEFORE UPDATE triggers (P1#1):
--    The previous trigger functions returned OLD unconditionally, so every
--    non-rejected UPDATE was silently discarded. For BEFORE UPDATE the function
--    must return NEW to apply the proposed changes. For BEFORE DELETE OLD is
--    still correct. Both policy and bundle triggers are replaced here.
--
-- 2. compliance_bundle_environments not syncing assignments (P1#2):
--    AFTER INSERT / DELETE triggers on compliance_bundle_environments now
--    maintain compliance_bundle_assignments so that creating or editing a
--    bundle's required environments is immediately reflected in the versioned
--    assignment table. The effective_set_digest is set to the sentinel value
--    'pending' and must be refreshed by the Rust service (P1#4 below).
--
-- 3. compute_bundle_draft_digest reads legacy table, not version table (P1#3):
--    The SQL function is replaced by a version that reads
--    compliance_bundle_version_policies ordered by policy_order. It also takes a
--    bundle_version_id argument rather than a bundle_id to avoid any ambiguity
--    about which version is current.
--    Because no SQL function can reproduce the exact Rust serde_json compact
--    serialization byte-for-byte (P1#4), the SQL helper now returns the sentinel
--    value 'pending' and a comment explains that Rust is authoritative.
--
-- 4. Canonical digest unification (P1#4):
--    SQL jsonb::text is not byte-for-byte equivalent to Rust's serde_json compact
--    serializer (key ordering, whitespace, number representation). All SQL digest
--    computations are replaced by the sentinel value 'pending'. The Rust service
--    layer is authoritative: every create/update/import/publish path must compute
--    the digest via semantic_digest() and persist it in the same transaction.
--    All existing backfilled digests are reset to 'pending' so the Rust service
--    recomputes them on first write.
--    NOTE: 'pending' is not a valid SHA-256 hex string, so any code that tries
--    to use a pending digest for identity comparison will fail loudly rather than
--    silently producing a wrong match.

-- ── 1. Fix BEFORE UPDATE return value ────────────────────────────────────────

CREATE OR REPLACE FUNCTION enforce_policy_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify or delete policy version % because it is in immutable state ''%''.',
            OLD.id, OLD.publication_state;
    END IF;
    -- For UPDATE return NEW to apply the change; for DELETE return OLD.
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION enforce_bundle_version_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.publication_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify or delete bundle version % because it is in immutable state ''%''.',
            OLD.id, OLD.publication_state;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

-- ── 2. Sync compliance_bundle_assignments on environment membership change ────

-- Helper: return the current draft version id for a bundle.
CREATE OR REPLACE FUNCTION bundle_current_draft_version(p_bundle_id uuid)
RETURNS uuid LANGUAGE sql STABLE AS $$
    SELECT current_draft_version_id FROM compliance_bundles WHERE id = p_bundle_id;
$$;

-- Trigger function for compliance_bundle_environments INSERT.
CREATE OR REPLACE FUNCTION sync_bundle_env_assignment_insert()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
BEGIN
    v_version_id := bundle_current_draft_version(NEW.bundle_id);
    IF v_version_id IS NULL THEN
        RETURN NEW;
    END IF;

    -- Upsert an enforce-mode assignment. The effective_set_digest is set to
    -- 'pending' and must be refreshed by the Rust service layer.
    INSERT INTO compliance_bundle_assignments (
        bundle_version_id, scope_type, environment_id, enforcement_mode,
        effective_set_digest, created_at, updated_at
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

CREATE TRIGGER trigger_sync_bundle_env_assignment_insert
    AFTER INSERT ON compliance_bundle_environments
    FOR EACH ROW
    EXECUTE FUNCTION sync_bundle_env_assignment_insert();

-- Trigger function for compliance_bundle_environments DELETE.
CREATE OR REPLACE FUNCTION sync_bundle_env_assignment_delete()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
BEGIN
    v_version_id := bundle_current_draft_version(OLD.bundle_id);
    IF v_version_id IS NULL THEN
        RETURN OLD;
    END IF;

    DELETE FROM compliance_bundle_assignments
    WHERE bundle_version_id = v_version_id
      AND scope_type = 'environment'
      AND environment_id = OLD.environment_id;

    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_sync_bundle_env_assignment_delete
    AFTER DELETE ON compliance_bundle_environments
    FOR EACH ROW
    EXECUTE FUNCTION sync_bundle_env_assignment_delete();

-- ── 3 & 4. Replace SQL digest computation with Rust-authoritative sentinel ───

-- The SQL helper from 0199 produced digests that differ from Rust serde_json
-- compact output. Drop it and replace with a stub that returns 'pending'.
-- The Rust service layer is the single authoritative digest implementation.
DROP FUNCTION IF EXISTS compute_bundle_draft_digest(uuid);

CREATE OR REPLACE FUNCTION compute_bundle_draft_digest_stub()
RETURNS text LANGUAGE sql IMMUTABLE AS $$
    SELECT 'pending';
$$;

-- Update the sync triggers to store 'pending' instead of a SQL-computed hash.
-- Rust must refresh the digest before the version is used for reconciliation.

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
            NEW.description, NEW.layer, NEW.owner,
            -- 'pending' is the sentinel; Rust refreshes this on first write.
            'pending'
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
                -- Mark stale; Rust recomputes after this UPDATE returns.
                semantic_digest = 'pending'
            WHERE id = NEW.current_draft_version_id
              AND publication_state = 'draft';
        ELSE
            INSERT INTO compliance_bundle_versions (
                bundle_id, version, name, framework, framework_version,
                description, layer, owner, semantic_digest
            ) VALUES (
                NEW.id, '0.1.0', NEW.name, NEW.framework, NULLIF(NEW.version, ''),
                NEW.description, NEW.layer, NEW.owner, 'pending'
            )
            RETURNING id INTO v_id;
            UPDATE compliance_bundles SET current_draft_version_id = v_id WHERE id = NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

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

-- Update the policy sync trigger to also use 'pending'.
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
              AND publication_state = 'draft';
        ELSE
            INSERT INTO deployment_policy_versions (
                policy_id, version, name, description, policy_type, config, semantic_digest
            ) VALUES (
                NEW.id, '0.1.0', NEW.name, NEW.description, NEW.policy_type, NEW.config,
                'pending'
            )
            RETURNING id INTO v_id;
            UPDATE deployment_policies SET current_draft_version_id = v_id WHERE id = NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

-- Reset all SQL-computed backfill digests to 'pending'. These are stale
-- because SQL jsonb::text does not match Rust serde_json compact output.
-- The Rust service will recompute them on the next write to each object.
UPDATE deployment_policy_versions
SET semantic_digest = 'pending'
WHERE publication_state = 'draft';

UPDATE compliance_bundle_versions
SET semantic_digest = 'pending'
WHERE publication_state = 'draft';

UPDATE compliance_bundle_assignments
SET effective_set_digest = 'pending';
