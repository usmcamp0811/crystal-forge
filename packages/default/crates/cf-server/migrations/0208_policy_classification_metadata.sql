-- Policy classification metadata (category, framework, severity, control_family, cmmc_level,
-- cis_section, rationale) is stored in the existing compliance_metadata JSONB column on
-- deployment_policy_versions. No schema change is required; this migration documents the
-- new keys and adds a GIN index to support future classification queries.

-- Index for efficient category queries without a full scan.
CREATE INDEX IF NOT EXISTS idx_dpv_classification
    ON deployment_policy_versions
    USING gin (compliance_metadata jsonb_path_ops);
