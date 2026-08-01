-- Migration 0197: Versioned bundle assignment overlays and import provenance.

CREATE TABLE compliance_source_object_mappings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_artifact_id uuid NOT NULL REFERENCES compliance_source_artifacts(id) ON DELETE CASCADE,
    object_kind text NOT NULL,
    source_identity text NOT NULL,
    policy_version_id uuid REFERENCES deployment_policy_versions(id) ON DELETE SET NULL,
    bundle_version_id uuid REFERENCES compliance_bundle_versions(id) ON DELETE SET NULL,
    fidelity text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (object_kind IN ('benchmark', 'profile', 'group', 'rule', 'value')),
    CHECK (fidelity IN ('native_exact', 'normalized_complete', 'preserved_opaque', 'degraded')),
    UNIQUE (source_artifact_id, object_kind, source_identity)
);

CREATE TABLE compliance_bundle_assignments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    bundle_version_id uuid NOT NULL REFERENCES compliance_bundle_versions(id) ON DELETE RESTRICT,
    scope_type text NOT NULL,
    environment_id uuid REFERENCES environments(id) ON DELETE CASCADE,
    system_id uuid REFERENCES systems(id) ON DELETE CASCADE,
    enforcement_mode text NOT NULL DEFAULT 'enforce',
    effective_set_digest text NOT NULL,
    provenance jsonb NOT NULL DEFAULT '{}',
    created_by uuid REFERENCES users(id),
    updated_by uuid REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (scope_type IN ('environment', 'system')),
    CHECK (enforcement_mode IN ('enforce', 'report_only')),
    CHECK (
        (scope_type = 'environment' AND environment_id IS NOT NULL AND system_id IS NULL)
        OR (scope_type = 'system' AND system_id IS NOT NULL AND environment_id IS NULL)
    )
);

CREATE UNIQUE INDEX compliance_bundle_assignments_environment_unique
    ON compliance_bundle_assignments (bundle_version_id, environment_id)
    WHERE environment_id IS NOT NULL;
CREATE UNIQUE INDEX compliance_bundle_assignments_system_unique
    ON compliance_bundle_assignments (bundle_version_id, system_id)
    WHERE system_id IS NOT NULL;

CREATE TRIGGER trigger_compliance_bundle_assignments_updated_at
    BEFORE UPDATE ON compliance_bundle_assignments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TABLE compliance_assignment_exclusions (
    assignment_id uuid NOT NULL REFERENCES compliance_bundle_assignments(id) ON DELETE CASCADE,
    policy_version_id uuid NOT NULL REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    PRIMARY KEY (assignment_id, policy_version_id)
);

CREATE TABLE compliance_assignment_additions (
    assignment_id uuid NOT NULL REFERENCES compliance_bundle_assignments(id) ON DELETE CASCADE,
    policy_version_id uuid NOT NULL REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    PRIMARY KEY (assignment_id, policy_version_id)
);

CREATE TABLE compliance_assignment_value_overrides (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    assignment_id uuid NOT NULL REFERENCES compliance_bundle_assignments(id) ON DELETE CASCADE,
    policy_version_id uuid NOT NULL REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    value_path text NOT NULL,
    value jsonb NOT NULL,
    UNIQUE (assignment_id, policy_version_id, value_path),
    CHECK (btrim(value_path) <> '')
);

-- Preserve legacy required-environment bundle semantics as enforce-mode
-- assignments to the initial backfilled bundle version.
INSERT INTO compliance_bundle_assignments (
    bundle_version_id, scope_type, environment_id, enforcement_mode, effective_set_digest, created_at, updated_at
)
SELECT version.id, 'environment', legacy.environment_id, 'enforce', version.semantic_digest,
       legacy.created_at, legacy.created_at
FROM compliance_bundle_environments legacy
JOIN compliance_bundle_versions version
    ON version.bundle_id = legacy.bundle_id AND version.version = '0.1.0'
ON CONFLICT DO NOTHING;
