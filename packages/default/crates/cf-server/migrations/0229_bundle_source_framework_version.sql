-- Authoritative source framework release for a bundle version.
--
-- DISA STIG bundles must always carry normalized requirement membership, but
-- the coverage report needs a bundle -> framework identity that survives the
-- exact corruption state this identity exists to diagnose (zero selected
-- requirement rows). Requirement membership alone cannot supply that
-- identity -- with zero membership rows it is empty by construction -- so the
-- bundle version records the framework release it was normalized against
-- directly. Populated by commit_foreign_import for STIG-classified imports;
-- NULL for bundles created without normalized framework requirements.
ALTER TABLE compliance_bundle_versions
    ADD COLUMN framework_version_id uuid
        REFERENCES compliance_framework_versions(id) ON DELETE SET NULL;

CREATE INDEX idx_compliance_bundle_versions_framework_version_id
    ON compliance_bundle_versions (framework_version_id);