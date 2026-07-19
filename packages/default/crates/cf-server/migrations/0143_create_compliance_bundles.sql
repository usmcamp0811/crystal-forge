-- Migration 0143: Compliance bundles
--
-- Bundles group existing deployment policies into reviewable compliance views.
-- Rollups/evidence are derived at read time from existing system, policy, and
-- CVE data; this schema only persists the bundle catalog and its associations.

CREATE TABLE IF NOT EXISTS compliance_bundles (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL UNIQUE,
    framework text NOT NULL,
    version text NOT NULL DEFAULT '',
    description text,
    layer text NOT NULL DEFAULT 'fleet',
    owner text NOT NULL DEFAULT 'Platform Security',
    last_review timestamptz,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT compliance_bundles_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT compliance_bundles_framework_not_blank CHECK (btrim(framework) <> ''),
    CONSTRAINT compliance_bundles_layer_not_blank CHECK (btrim(layer) <> '')
);

CREATE TRIGGER trigger_compliance_bundles_updated_at
    BEFORE UPDATE ON compliance_bundles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TABLE IF NOT EXISTS compliance_bundle_policies (
    bundle_id uuid NOT NULL REFERENCES compliance_bundles(id) ON DELETE CASCADE,
    policy_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bundle_id, policy_id)
);

CREATE INDEX IF NOT EXISTS idx_compliance_bundle_policies_policy
    ON compliance_bundle_policies(policy_id);

CREATE TABLE IF NOT EXISTS compliance_bundle_environments (
    bundle_id uuid NOT NULL REFERENCES compliance_bundles(id) ON DELETE CASCADE,
    environment_id uuid NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bundle_id, environment_id)
);

CREATE INDEX IF NOT EXISTS idx_compliance_bundle_environments_environment
    ON compliance_bundle_environments(environment_id);
