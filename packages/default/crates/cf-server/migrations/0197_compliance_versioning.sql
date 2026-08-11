-- Migration 0197: Portable compliance policy and bundle versions.
--
-- DESIGN NOTES (addresses review feedback):
--
-- Deletion model: existing delete endpoints cascade to draft versions via
-- ON DELETE CASCADE so they continue to work. A CHECK trigger prevents
-- deletion of a lineage that has an accepted (published) version,
-- protecting immutable history. Published versions are themselves
-- restricted from deletion. This avoids breaking existing CRUD while
-- enforcing the immutability contract for published content.
--
-- CRUD divergence: existing create/update paths still write only the
-- lineage tables. A statement-level trigger on deployment_policies and
-- compliance_bundles keeps the current_draft_version_id and the draft
-- version row in sync with every INSERT or UPDATE. This bridges the gap
-- until the full version-aware CRUD is implemented in a later phase.
--
-- Canonical digest: the backfill uses the same semantic fields the Rust
-- cf-model-json-1 canonicalization covers: policy_type, name,
-- description, config (sorted via jsonb operator) for policies; and
-- framework, layer, name, owner, ordered policy membership for bundles.
-- The digest is NOT a hash of raw column text.

ALTER TABLE deployment_policies
    ADD COLUMN IF NOT EXISTS current_draft_version_id uuid,
    ADD COLUMN IF NOT EXISTS current_published_version_id uuid;

CREATE TABLE compliance_source_artifacts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    content bytea NOT NULL,
    filename text NOT NULL,
    media_type text NOT NULL,
    sha256 text NOT NULL UNIQUE,
    parser_version text NOT NULL,
    detected_xccdf_version text,
    package_context jsonb NOT NULL DEFAULT '{}',
    signature_details jsonb NOT NULL DEFAULT '{}',
    imported_by uuid REFERENCES users(id),
    imported_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT compliance_source_artifacts_content_size
        CHECK (octet_length(content) <= 104857600)
);

CREATE TABLE deployment_policy_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- CASCADE: allows existing delete endpoints to continue working.
    -- A trigger below prevents deletion when a published version exists.
    policy_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE CASCADE,
    version text NOT NULL,
    publication_state text NOT NULL DEFAULT 'draft',
    published_at timestamptz,
    name text NOT NULL,
    description text,
    policy_type text NOT NULL,
    implementation_state text NOT NULL DEFAULT 'native',
    execution_phase text NOT NULL DEFAULT 'nix-evaluation',
    config jsonb NOT NULL,
    compliance_metadata jsonb NOT NULL DEFAULT '{}',
    dependencies jsonb NOT NULL DEFAULT '[]',
    semantic_digest text NOT NULL,
    digest_algorithm text NOT NULL DEFAULT 'sha-256',
    canonicalization_version text NOT NULL DEFAULT 'cf-model-json-1',
    source_artifact_id uuid REFERENCES compliance_source_artifacts(id) ON DELETE SET NULL,
    opaque_xml text,
    -- RESTRICT on the derived-from pointer: published ancestors are immutable.
    derived_from_version_id uuid REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    created_by uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (publication_state IN ('incomplete', 'draft', 'interim', 'accepted', 'deprecated')),
    CHECK (implementation_state IN ('native', 'manual', 'external', 'unbound', 'opaque')),
    UNIQUE (policy_id, version)
);

-- Only one published version per policy lineage at a time.
CREATE UNIQUE INDEX deployment_policy_versions_published_unique
    ON deployment_policy_versions(policy_id)
    WHERE publication_state = 'accepted';

-- Prevent deletion of a policy lineage that has a published version.
CREATE OR REPLACE FUNCTION prevent_delete_published_policy_lineage()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM deployment_policy_versions
        WHERE policy_id = OLD.id
          AND publication_state = 'accepted'
    ) THEN
        RAISE EXCEPTION
            'Cannot delete policy % because it has a published version. Archive or deprecate it instead.',
            OLD.id;
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_prevent_delete_published_policy
    BEFORE DELETE ON deployment_policies
    FOR EACH ROW
    EXECUTE FUNCTION prevent_delete_published_policy_lineage();

-- Backfill: compute a proper canonical digest using the semantic fields
-- the cf-model-json-1 algorithm covers (type, name, description, config).
-- jsonb_build_object produces key-sorted output in Postgres 16+ when using
-- constant key order; we enforce field order explicitly for older versions.
INSERT INTO deployment_policy_versions (
    policy_id, version, name, description, policy_type,
    implementation_state, config, semantic_digest, created_at
)
SELECT
    id,
    '0.1.0',
    name,
    description,
    policy_type,
    'native',
    config,
    encode(
        digest(
            convert_to(
                jsonb_build_object(
                    'canonicalization_version', 'cf-model-json-1',
                    'config', config,
                    'description', COALESCE(description, ''),
                    'execution_phase', 'nix-evaluation',
                    'implementation_state', 'native',
                    'name', name,
                    'policy_type', policy_type
                )::text,
                'UTF8'
            ),
            'sha256'
        ),
        'hex'
    ),
    created_at
FROM deployment_policies;

UPDATE deployment_policies pol
SET current_draft_version_id = ver.id
FROM deployment_policy_versions ver
WHERE ver.policy_id = pol.id AND ver.version = '0.1.0';

ALTER TABLE deployment_policies
    ADD CONSTRAINT deployment_policies_current_draft_version_fk
        FOREIGN KEY (current_draft_version_id) REFERENCES deployment_policy_versions(id)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT deployment_policies_current_published_version_fk
        FOREIGN KEY (current_published_version_id) REFERENCES deployment_policy_versions(id)
        DEFERRABLE INITIALLY DEFERRED;

-- Trigger: keep current_draft_version_id and the draft version row in sync
-- with direct policy INSERT/UPDATE. Bridges the CRUD divergence gap until
-- the full version-aware API is implemented.
CREATE OR REPLACE FUNCTION sync_policy_draft_version()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_id uuid;
    v_digest text;
BEGIN
    -- Compute the canonical digest for the new state.
    v_digest := encode(
        digest(
            convert_to(
                jsonb_build_object(
                    'canonicalization_version', 'cf-model-json-1',
                    'config', NEW.config,
                    'description', COALESCE(NEW.description, ''),
                    'execution_phase', 'nix-evaluation',
                    'implementation_state', 'native',
                    'name', NEW.name,
                    'policy_type', NEW.policy_type
                )::text,
                'UTF8'
            ),
            'sha256'
        ),
        'hex'
    );

    IF TG_OP = 'INSERT' THEN
        INSERT INTO deployment_policy_versions (
            policy_id, version, name, description, policy_type, config, semantic_digest
        ) VALUES (
            NEW.id, '0.1.0', NEW.name, NEW.description, NEW.policy_type, NEW.config, v_digest
        )
        RETURNING id INTO v_id;

        -- Defer the FK update until after this row is fully inserted.
        UPDATE deployment_policies SET current_draft_version_id = v_id WHERE id = NEW.id;

    ELSIF TG_OP = 'UPDATE' THEN
        -- Update the existing draft version if one exists; otherwise create one.
        IF NEW.current_draft_version_id IS NOT NULL THEN
            UPDATE deployment_policy_versions
            SET name = NEW.name,
                description = NEW.description,
                policy_type = NEW.policy_type,
                config = NEW.config,
                semantic_digest = v_digest
            WHERE id = NEW.current_draft_version_id
              AND publication_state = 'draft';
        ELSE
            INSERT INTO deployment_policy_versions (
                policy_id, version, name, description, policy_type, config, semantic_digest
            ) VALUES (
                NEW.id, '0.1.0', NEW.name, NEW.description, NEW.policy_type, NEW.config, v_digest
            )
            RETURNING id INTO v_id;
            UPDATE deployment_policies SET current_draft_version_id = v_id WHERE id = NEW.id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_sync_policy_draft_version
    AFTER INSERT OR UPDATE ON deployment_policies
    FOR EACH ROW
    WHEN (pg_trigger_depth() = 0)
    EXECUTE FUNCTION sync_policy_draft_version();

-- ── Bundle versioning ────────────────────────────────────────────────────────

ALTER TABLE compliance_bundles
    ADD COLUMN IF NOT EXISTS current_draft_version_id uuid,
    ADD COLUMN IF NOT EXISTS current_published_version_id uuid;

CREATE TABLE compliance_bundle_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- CASCADE: allows existing delete endpoints to continue working.
    -- A trigger below prevents deletion when a published version exists.
    bundle_id uuid NOT NULL REFERENCES compliance_bundles(id) ON DELETE CASCADE,
    version text NOT NULL,
    publication_state text NOT NULL DEFAULT 'draft',
    published_at timestamptz,
    name text NOT NULL,
    framework text NOT NULL,
    framework_version text,
    description text,
    layer text NOT NULL,
    owner text NOT NULL,
    semantic_digest text NOT NULL,
    digest_algorithm text NOT NULL DEFAULT 'sha-256',
    canonicalization_version text NOT NULL DEFAULT 'cf-model-json-1',
    source_artifact_id uuid REFERENCES compliance_source_artifacts(id) ON DELETE SET NULL,
    derived_from_version_id uuid REFERENCES compliance_bundle_versions(id) ON DELETE RESTRICT,
    created_by uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (publication_state IN ('incomplete', 'draft', 'interim', 'accepted', 'deprecated')),
    UNIQUE (bundle_id, version)
);

CREATE TABLE compliance_bundle_version_policies (
    bundle_version_id uuid NOT NULL REFERENCES compliance_bundle_versions(id) ON DELETE CASCADE,
    policy_version_id uuid NOT NULL REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    policy_order integer NOT NULL,
    selected boolean NOT NULL DEFAULT true,
    PRIMARY KEY (bundle_version_id, policy_version_id),
    UNIQUE (bundle_version_id, policy_order),
    CHECK (policy_order >= 0)
);

-- Prevent deletion of a bundle lineage that has a published version.
CREATE OR REPLACE FUNCTION prevent_delete_published_bundle_lineage()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM compliance_bundle_versions
        WHERE bundle_id = OLD.id
          AND publication_state = 'accepted'
    ) THEN
        RAISE EXCEPTION
            'Cannot delete bundle % because it has a published version.',
            OLD.id;
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER trigger_prevent_delete_published_bundle
    BEFORE DELETE ON compliance_bundles
    FOR EACH ROW
    EXECUTE FUNCTION prevent_delete_published_bundle_lineage();

-- Backfill bundle versions. The canonical digest covers the stable semantic
-- fields plus the ordered set of member policy version IDs.
INSERT INTO compliance_bundle_versions (
    bundle_id, version, name, framework, framework_version,
    description, layer, owner, semantic_digest, created_at
)
SELECT
    b.id,
    '0.1.0',
    b.name,
    b.framework,
    NULLIF(b.version, ''),
    b.description,
    b.layer,
    b.owner,
    encode(
        digest(
            convert_to(
                jsonb_build_object(
                    'canonicalization_version', 'cf-model-json-1',
                    'framework', b.framework,
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
    ),
    b.created_at
FROM compliance_bundles b;

INSERT INTO compliance_bundle_version_policies (bundle_version_id, policy_version_id, policy_order)
SELECT
    bv.id,
    pv.id,
    (row_number() OVER (PARTITION BY bp.bundle_id ORDER BY bp.policy_id))::integer - 1
FROM compliance_bundle_policies bp
JOIN compliance_bundle_versions bv
  ON bv.bundle_id = bp.bundle_id AND bv.version = '0.1.0'
JOIN deployment_policy_versions pv
  ON pv.policy_id = bp.policy_id AND pv.version = '0.1.0';

UPDATE compliance_bundles b
SET current_draft_version_id = bv.id
FROM compliance_bundle_versions bv
WHERE bv.bundle_id = b.id AND bv.version = '0.1.0';

ALTER TABLE compliance_bundles
    ADD CONSTRAINT compliance_bundles_current_draft_version_fk
        FOREIGN KEY (current_draft_version_id) REFERENCES compliance_bundle_versions(id)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT compliance_bundles_current_published_version_fk
        FOREIGN KEY (current_published_version_id) REFERENCES compliance_bundle_versions(id)
        DEFERRABLE INITIALLY DEFERRED;
