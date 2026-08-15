-- ── TASK-418: Compliance Framework Lineages and Versions ──────────────────────
--
-- Introduces first-class framework lineages and immutable framework version
-- records.  Frameworks are authoritative compliance standards (NIST 800-53,
-- DISA STIGs, CIS Benchmarks, …).  Interchange formats such as XCCDF are
-- *not* frameworks.
--
-- Uniqueness model:
--   compliance_frameworks:         UNIQUE (canonical_source_key)
--   compliance_framework_versions: UNIQUE (framework_id, canonical_release_key)
--
-- A duplicate canonical_release_key with a different semantic content must be
-- rejected by application code with a typed FRAMEWORK_RELEASE_CONFLICT error,
-- never silently accepted.
--
-- Semantic digest:
--   semantic_digest is set to 'pending' by Rust after initial insert and
--   recomputed (cf-model-json-1 / sha-256) within the same transaction.
--   The startup backfill in digest.rs handles any rows that missed Rust writes.

-- ── 1. Framework lineages ─────────────────────────────────────────────────────

CREATE TABLE compliance_frameworks (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name                 TEXT        NOT NULL,
    publisher            TEXT,
    -- Machine-stable key that uniquely identifies this framework across
    -- installations, e.g. "disa-anduril-nixos-stig", "nist-800-53".
    -- Determined by the import adapter; never derived from display text.
    canonical_source_key TEXT        NOT NULL,
    description          TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT compliance_frameworks_canonical_key_unique
        UNIQUE (canonical_source_key)
);

COMMENT ON TABLE compliance_frameworks IS
    'Authoritative compliance framework lineages (NIST 800-53, DISA STIGs, …). '
    'One row per distinct framework regardless of how many releases exist.';

COMMENT ON COLUMN compliance_frameworks.canonical_source_key IS
    'Stable adapter-determined key that uniquely identifies the framework, '
    'e.g. "disa-anduril-nixos-stig". Never derived from display names.';

-- ── 2. Framework versions (immutable releases) ────────────────────────────────

CREATE TABLE compliance_framework_versions (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    framework_id         UUID        NOT NULL
                             REFERENCES compliance_frameworks (id)
                             ON DELETE RESTRICT,
    -- Human-readable version string, e.g. "Rev 5", "V1R1", "V1R2".
    version              TEXT        NOT NULL,
    -- Adapter-determined release identifier used for uniqueness checking,
    -- e.g. "V1R1".  Compared case-insensitively by application code.
    canonical_release_key TEXT       NOT NULL,
    title                TEXT,
    published_at         TIMESTAMPTZ,
    -- Optional: the source artifact from which this version was imported.
    source_artifact_id   UUID
                             REFERENCES compliance_source_artifacts (id)
                             ON DELETE RESTRICT,
    -- Digest over: framework_id, canonical_release_key, title, published_at,
    -- and derived requirement semantic digests.
    -- Rust sets this to 'pending' then replaces it within the same transaction.
    semantic_digest      TEXT        NOT NULL DEFAULT 'pending',
    digest_algorithm     TEXT        NOT NULL DEFAULT 'sha-256',
    canonicalization_version TEXT   NOT NULL DEFAULT 'cf-model-json-1',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT compliance_framework_versions_release_unique
        UNIQUE (framework_id, canonical_release_key)
);

COMMENT ON TABLE compliance_framework_versions IS
    'Immutable releases of a compliance framework, e.g. NIST 800-53 Rev 5, '
    'DISA Anduril NixOS STIG V1R1.  A framework_id + canonical_release_key '
    'pair is globally unique; attempting to import a duplicate with different '
    'content must be rejected with FRAMEWORK_RELEASE_CONFLICT.';

COMMENT ON COLUMN compliance_framework_versions.canonical_release_key IS
    'Adapter-determined release identifier used for uniqueness checking. '
    'Application code compares this case-insensitively.';

COMMENT ON COLUMN compliance_framework_versions.semantic_digest IS
    'cf-model-json-1/sha-256 digest. Rust writes the real value immediately '
    'after insert (within the same transaction).  The pending sentinel is '
    'kept only until Rust has written the real value.';

-- Index to accelerate "find all versions of framework X" queries.
CREATE INDEX idx_compliance_framework_versions_framework_id
    ON compliance_framework_versions (framework_id);
