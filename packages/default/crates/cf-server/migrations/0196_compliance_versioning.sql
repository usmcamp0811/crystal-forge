-- Migration 0196: Portable, immutable compliance policy and bundle versions.
-- Existing policy and bundle IDs remain stable lineage IDs; current state is
-- backfilled as an initial mutable draft without changing existing assignments.

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
    policy_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE RESTRICT,
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
    derived_from_version_id uuid REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    created_by uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (publication_state IN ('incomplete', 'draft', 'interim', 'accepted', 'deprecated')),
    CHECK (implementation_state IN ('native', 'manual', 'external', 'unbound', 'opaque')),
    UNIQUE (policy_id, version)
);

CREATE UNIQUE INDEX deployment_policy_versions_published_name
    ON deployment_policy_versions(policy_id)
    WHERE publication_state = 'accepted';

INSERT INTO deployment_policy_versions (
    policy_id, version, name, description, policy_type, config, semantic_digest, created_at
)
SELECT id, '0.1.0', name, description, policy_type, config,
       encode(digest(convert_to(config::text, 'UTF8'), 'sha256'), 'hex'), created_at
FROM deployment_policies;

UPDATE deployment_policies policy
SET current_draft_version_id = version.id
FROM deployment_policy_versions version
WHERE version.policy_id = policy.id AND version.version = '0.1.0';

ALTER TABLE deployment_policies
    ADD CONSTRAINT deployment_policies_current_draft_version_fk
    FOREIGN KEY (current_draft_version_id) REFERENCES deployment_policy_versions(id),
    ADD CONSTRAINT deployment_policies_current_published_version_fk
    FOREIGN KEY (current_published_version_id) REFERENCES deployment_policy_versions(id);

ALTER TABLE compliance_bundles
    ADD COLUMN IF NOT EXISTS current_draft_version_id uuid,
    ADD COLUMN IF NOT EXISTS current_published_version_id uuid;

CREATE TABLE compliance_bundle_versions (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    bundle_id uuid NOT NULL REFERENCES compliance_bundles(id) ON DELETE RESTRICT,
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

INSERT INTO compliance_bundle_versions (
    bundle_id, version, name, framework, framework_version, description, layer, owner, semantic_digest, created_at
)
SELECT id, '0.1.0', name, framework, NULLIF(version, ''), description, layer, owner,
       encode(digest(convert_to(jsonb_build_object('framework', framework, 'name', name, 'policy_ids', '[]'::jsonb)::text, 'UTF8'), 'sha256'), 'hex'), created_at
FROM compliance_bundles;

INSERT INTO compliance_bundle_version_policies (bundle_version_id, policy_version_id, policy_order)
SELECT bundle_version.id, policy_version.id,
       row_number() OVER (PARTITION BY membership.bundle_id ORDER BY membership.policy_id) - 1
FROM compliance_bundle_policies membership
JOIN compliance_bundle_versions bundle_version ON bundle_version.bundle_id = membership.bundle_id AND bundle_version.version = '0.1.0'
JOIN deployment_policies policy ON policy.id = membership.policy_id
JOIN deployment_policy_versions policy_version ON policy_version.id = policy.current_draft_version_id;

UPDATE compliance_bundles bundle
SET current_draft_version_id = version.id
FROM compliance_bundle_versions version
WHERE version.bundle_id = bundle.id AND version.version = '0.1.0';

ALTER TABLE compliance_bundles
    ADD CONSTRAINT compliance_bundles_current_draft_version_fk
    FOREIGN KEY (current_draft_version_id) REFERENCES compliance_bundle_versions(id),
    ADD CONSTRAINT compliance_bundles_current_published_version_fk
    FOREIGN KEY (current_published_version_id) REFERENCES compliance_bundle_versions(id);
