-- ── TASK-418: Compliance Requirement Lineages and Versions ────────────────────
--
-- Introduces stable requirement lineages and immutable requirement version
-- records.  A lineage (compliance_requirements) is identified by
-- (framework_id, canonical_requirement_key).  Each time a framework is
-- released the same requirement lineage may appear with updated content —
-- that produces a new compliance_requirement_versions row on the same lineage.
--
-- Hierarchy:
--   parent_requirement_version_id → generic parent within the same release.
--   kind                          → framework-specific node type
--                                   (family/control/enhancement for NIST;
--                                    group/rule for DISA STIG;
--                                    section/subsection/recommendation for CIS).
--   Depth is not limited; the UI renders arbitrary hierarchy via recursion.
--
-- Uniqueness model:
--   compliance_requirements:         UNIQUE (framework_id, canonical_requirement_key)
--   compliance_requirement_versions: UNIQUE (requirement_id, framework_version_id)
--
-- Semantic digest:
--   Same 'pending' sentinel and Rust-computes-in-transaction convention as
--   compliance_framework_versions.

-- ── 1. Requirement lineages ───────────────────────────────────────────────────

CREATE TABLE compliance_requirements (
    id                       UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    framework_id             UUID        NOT NULL
                                 REFERENCES compliance_frameworks (id)
                                 ON DELETE RESTRICT,
    -- Adapter-determined stable identifier for this requirement within its
    -- framework, e.g. "V-268137" (DISA STIG), "SC-45" (NIST), "5.1.8" (CIS).
    -- Preferred to the release-specific external_id when a stable identifier
    -- is available.
    canonical_requirement_key TEXT       NOT NULL,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT compliance_requirements_key_unique
        UNIQUE (framework_id, canonical_requirement_key)
);

COMMENT ON TABLE compliance_requirements IS
    'Stable compliance requirement lineages.  One row per distinct requirement '
    'regardless of how many framework releases have included it.';

COMMENT ON COLUMN compliance_requirements.canonical_requirement_key IS
    'Adapter-determined stable identifier within its framework, e.g. "V-268137" '
    'for DISA STIG rules or "SC-45" for NIST controls. '
    'Must not change when the requirement reappears in a new release.';

-- Index to enumerate all requirements for a framework efficiently.
CREATE INDEX idx_compliance_requirements_framework_id
    ON compliance_requirements (framework_id);

-- ── 2. Requirement versions (immutable per release) ───────────────────────────

CREATE TABLE compliance_requirement_versions (
    id                           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    requirement_id               UUID        NOT NULL
                                     REFERENCES compliance_requirements (id)
                                     ON DELETE RESTRICT,
    framework_version_id         UUID        NOT NULL
                                     REFERENCES compliance_framework_versions (id)
                                     ON DELETE RESTRICT,
    -- The identifier used in the source artifact for this release, e.g. the
    -- XCCDF Rule id or NIST control number.  May differ from canonical_requirement_key
    -- if the release uses a release-specific form.
    external_id                  TEXT        NOT NULL,
    title                        TEXT,
    description                  TEXT,
    -- Generic node type within the framework hierarchy.
    -- Examples: family/control/enhancement (NIST), group/rule (DISA STIG),
    -- section/subsection/recommendation (CIS), domain/practice (CMMC).
    kind                         TEXT        NOT NULL,
    -- Parent node within the same framework release.  NULL for root nodes.
    parent_requirement_version_id UUID
                                     REFERENCES compliance_requirement_versions (id)
                                     ON DELETE RESTRICT
                                     DEFERRABLE INITIALLY DEFERRED,
    severity                     TEXT,
    check_text                   TEXT,
    fix_text                     TEXT,
    -- Framework-specific supplementary data: CCI IDs, SRG IDs, references,
    -- platform applicability, version strings, legacy identifiers, etc.
    metadata                     JSONB       NOT NULL DEFAULT '{}',
    -- cf-model-json-1/sha-256 digest of the canonical representation.
    -- Rust writes the real value within the same transaction; 'pending' is
    -- the sentinel until then.
    semantic_digest              TEXT        NOT NULL DEFAULT 'pending',
    digest_algorithm             TEXT        NOT NULL DEFAULT 'sha-256',
    canonicalization_version     TEXT        NOT NULL DEFAULT 'cf-model-json-1',
    created_at                   TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- One version per release per lineage.
    CONSTRAINT compliance_requirement_versions_release_unique
        UNIQUE (requirement_id, framework_version_id)
);

COMMENT ON TABLE compliance_requirement_versions IS
    'Immutable snapshots of a compliance requirement as it appeared in a specific '
    'framework release.  Comparing semantic_digest across releases allows CF to '
    'determine whether the requirement changed.';

COMMENT ON COLUMN compliance_requirement_versions.kind IS
    'Generic hierarchy node type, e.g. family/control/enhancement for NIST, '
    'group/rule for DISA STIG.  Not interpreted by the DB; used for UI grouping.';

COMMENT ON COLUMN compliance_requirement_versions.parent_requirement_version_id IS
    'Parent node within the same framework release, or NULL for root-level nodes. '
    'DEFERRABLE because self-referential inserts in one batch need deferred checks.';

COMMENT ON COLUMN compliance_requirement_versions.metadata IS
    'Framework-specific supplementary identifiers and attributes, e.g. '
    '{"cci_ids":["CCI-000770"],"srg_ids":["SRG-OS-000109"]}.';

COMMENT ON COLUMN compliance_requirement_versions.semantic_digest IS
    'cf-model-json-1/sha-256 digest. Rust writes the real value immediately '
    'after insert within the same transaction.';

-- Index to find all versions of a requirement across releases.
CREATE INDEX idx_compliance_requirement_versions_requirement_id
    ON compliance_requirement_versions (requirement_id);

-- Index to find all requirements in a framework version.
CREATE INDEX idx_compliance_requirement_versions_framework_version_id
    ON compliance_requirement_versions (framework_version_id);

-- Index to find children of a parent node.
CREATE INDEX idx_compliance_requirement_versions_parent_id
    ON compliance_requirement_versions (parent_requirement_version_id)
    WHERE parent_requirement_version_id IS NOT NULL;

-- GIN index on metadata JSONB for CCI/SRG look-ups.
CREATE INDEX idx_compliance_requirement_versions_metadata
    ON compliance_requirement_versions USING GIN (metadata);

-- Full-text search index covering external_id and title.
-- Used by the server-side requirement search endpoint.
CREATE INDEX idx_compliance_requirement_versions_fts
    ON compliance_requirement_versions
    USING GIN (to_tsvector('english',
        COALESCE(external_id, '') || ' ' || COALESCE(title, '')));
