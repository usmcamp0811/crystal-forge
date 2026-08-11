-- ── TASK-418: Policy-to-Requirement Mappings and Bundle Requirement Membership ─
--
-- Introduces two new join tables:
--
--   policy_requirement_mappings
--       First-class many-to-many relationship between exact policy versions and
--       exact requirement versions.  Supports relationship semantics
--       (implements/supports/provides_evidence_for), coverage (full/partial),
--       provenance (manual/imported/inherited/inferred/suggested), and
--       optional rationale text.
--
--   compliance_bundle_version_requirements
--       Explicit requirement membership for bundle versions, separate from
--       policy membership.  A bundle version thus has two independent sets:
--         1. Requirement baseline  (what the framework requires)
--         2. Policy set            (which technical implementations CF selected)
--       Neither requires the other to be non-empty.
--
-- Immutability:
--   Mappings on an accepted or deprecated policy version are write-protected by
--   trigger (guard_policy_mapping_immutability).  To modify mappings the caller
--   must first create a derived draft via the !313 derived-draft workflow.
--   Suggested mappings (trust_state = 'suggested') are held separately and must
--   be explicitly accepted before they are treated as authoritative.

-- ── 1. Policy-to-requirement mappings ────────────────────────────────────────

CREATE TABLE policy_requirement_mappings (
    id                     UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_version_id      UUID        NOT NULL
                               REFERENCES deployment_policy_versions (id)
                               ON DELETE RESTRICT,
    requirement_version_id UUID        NOT NULL
                               REFERENCES compliance_requirement_versions (id)
                               ON DELETE RESTRICT,
    -- Semantic relationship between the policy and the requirement.
    -- Allowed values: implements / supports / provides_evidence_for
    relationship           TEXT        NOT NULL,
    -- How much of the requirement this mapping covers.
    -- Allowed values: full / partial
    coverage               TEXT        NOT NULL,
    rationale              TEXT,
    -- How this mapping was established.
    -- Allowed values: manual / imported / inherited / inferred / suggested
    provenance             TEXT        NOT NULL DEFAULT 'manual',
    -- Source artifact from which an imported or inherited mapping was derived.
    source_artifact_id     UUID
                               REFERENCES compliance_source_artifacts (id)
                               ON DELETE SET NULL,
    -- trusted  = authoritative, included in coverage computation
    -- suggested = candidate awaiting explicit acceptance, excluded from coverage
    trust_state            TEXT        NOT NULL DEFAULT 'trusted',
    created_by             UUID
                               REFERENCES users (id)
                               ON DELETE SET NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- At most one mapping per (policy version, requirement version) pair.
    CONSTRAINT policy_requirement_mappings_unique
        UNIQUE (policy_version_id, requirement_version_id),
    -- Validate allowed values.
    CONSTRAINT policy_requirement_mappings_relationship_check
        CHECK (relationship IN ('implements', 'supports', 'provides_evidence_for')),
    CONSTRAINT policy_requirement_mappings_coverage_check
        CHECK (coverage IN ('full', 'partial')),
    CONSTRAINT policy_requirement_mappings_provenance_check
        CHECK (provenance IN ('manual', 'imported', 'inherited', 'inferred', 'suggested')),
    CONSTRAINT policy_requirement_mappings_trust_state_check
        CHECK (trust_state IN ('trusted', 'suggested'))
);

COMMENT ON TABLE policy_requirement_mappings IS
    'First-class many-to-many join between exact policy versions and exact '
    'requirement versions.  Mappings on accepted/deprecated policy versions are '
    'immutable (enforced by trigger).  Suggested mappings (trust_state=suggested) '
    'are excluded from authoritative coverage computation.';

COMMENT ON COLUMN policy_requirement_mappings.relationship IS
    'implements: policy directly satisfies the requirement. '
    'supports: policy contributes but does not independently satisfy the requirement. '
    'provides_evidence_for: policy gathers evidence relevant to the requirement.';

COMMENT ON COLUMN policy_requirement_mappings.provenance IS
    'How this mapping was established: manual (user action), imported (from source '
    'artifact), inherited (from previous requirement version), inferred (from '
    'technical candidate matching), suggested (crosswalk candidate not yet accepted).';

COMMENT ON COLUMN policy_requirement_mappings.trust_state IS
    'trusted: authoritative, included in coverage. '
    'suggested: candidate awaiting explicit acceptance, excluded from coverage.';

-- Indexes for the common read paths.
CREATE INDEX idx_policy_requirement_mappings_policy_version_id
    ON policy_requirement_mappings (policy_version_id);

CREATE INDEX idx_policy_requirement_mappings_requirement_version_id
    ON policy_requirement_mappings (requirement_version_id);

-- ── 2. Immutability trigger for policy mappings ───────────────────────────────
--
-- Prevents INSERT, UPDATE, or DELETE on policy_requirement_mappings when the
-- referenced policy version is in an immutable state (accepted/deprecated).
-- This follows the same guard pattern established by 0202 for bundle membership.

CREATE OR REPLACE FUNCTION guard_policy_mapping_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
    v_state      text;
BEGIN
    -- For INSERT/UPDATE use NEW; for DELETE use OLD.
    v_version_id := COALESCE(NEW.policy_version_id, OLD.policy_version_id);
    SELECT publication_state INTO v_state
    FROM deployment_policy_versions WHERE id = v_version_id;

    IF v_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify requirement mappings for policy version % because it '
            'is in immutable state ''%''. Create a derived draft first.',
            v_version_id, v_state;
    END IF;
    RETURN COALESCE(NEW, OLD);
END;
$$;

CREATE TRIGGER trigger_guard_policy_mapping_immutability
    BEFORE INSERT OR UPDATE OR DELETE ON policy_requirement_mappings
    FOR EACH ROW
    EXECUTE FUNCTION guard_policy_mapping_immutability();

-- ── 3. Bundle version requirement membership ──────────────────────────────────

CREATE TABLE compliance_bundle_version_requirements (
    bundle_version_id      UUID        NOT NULL
                               REFERENCES compliance_bundle_versions (id)
                               ON DELETE CASCADE,
    requirement_version_id UUID        NOT NULL
                               REFERENCES compliance_requirement_versions (id)
                               ON DELETE RESTRICT,
    selected               BOOLEAN     NOT NULL DEFAULT TRUE,
    -- Explicit display order within the bundle version.
    requirement_order      INTEGER     NOT NULL,
    PRIMARY KEY (bundle_version_id, requirement_version_id)
);

COMMENT ON TABLE compliance_bundle_version_requirements IS
    'Explicit requirement membership for bundle versions, separate from the '
    'policy membership in compliance_bundle_version_policies.  A bundle version '
    'has an independent requirement baseline (what the framework requires) and '
    'policy set (which implementations CF selected).  Neither implies the other.';

-- Index for "list requirements in bundle version" queries.
CREATE INDEX idx_cbvr_bundle_version_id
    ON compliance_bundle_version_requirements (bundle_version_id);

-- Index for "which bundles include this requirement" queries.
CREATE INDEX idx_cbvr_requirement_version_id
    ON compliance_bundle_version_requirements (requirement_version_id);

-- ── 4. Requirement membership immutability guard ───────────────────────────────
--
-- Mirrors guard_bundle_version_membership_immutability for the new table.

CREATE OR REPLACE FUNCTION guard_bundle_requirement_membership_immutability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_version_id uuid;
    v_state      text;
BEGIN
    v_version_id := COALESCE(NEW.bundle_version_id, OLD.bundle_version_id);
    SELECT publication_state INTO v_state
    FROM compliance_bundle_versions WHERE id = v_version_id;

    IF v_state IN ('accepted', 'deprecated') THEN
        RAISE EXCEPTION
            'Cannot modify requirement membership of bundle version % because it '
            'is in immutable state ''%''.',
            v_version_id, v_state;
    END IF;
    RETURN COALESCE(NEW, OLD);
END;
$$;

CREATE TRIGGER trigger_guard_bundle_requirement_membership_immutability
    BEFORE INSERT OR UPDATE OR DELETE ON compliance_bundle_version_requirements
    FOR EACH ROW
    EXECUTE FUNCTION guard_bundle_requirement_membership_immutability();
