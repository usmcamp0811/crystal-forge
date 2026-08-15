-- Keep the model semantic digest compatible with cf-model-json-1 while exposing
-- normalized mapping and requirement-baseline identity separately.

ALTER TABLE deployment_policy_versions
    ADD COLUMN mapping_digest TEXT NOT NULL DEFAULT 'pending';

ALTER TABLE compliance_bundle_versions
    ADD COLUMN requirement_digest TEXT NOT NULL DEFAULT 'pending';
