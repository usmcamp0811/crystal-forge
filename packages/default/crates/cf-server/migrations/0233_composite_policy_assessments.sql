CREATE TABLE composite_policy_assessments (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    derivation_id integer NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    target_store_path text NOT NULL,
    policy_lineage_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE RESTRICT,
    policy_version_id uuid NOT NULL REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    effective_set_digest text NOT NULL,
    effective_config_digest text NOT NULL,
    effective_config jsonb NOT NULL,
    overall_outcome text NOT NULL DEFAULT 'not_checked',
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (overall_outcome IN ('pass', 'fail', 'error', 'not_checked')),
    CONSTRAINT composite_policy_assessments_exact_context_unique UNIQUE (
        system_id,
        derivation_id,
        target_store_path,
        policy_version_id,
        effective_set_digest
    )
);

CREATE TABLE composite_policy_derivation_targets (
    derivation_id integer NOT NULL REFERENCES derivations(id) ON DELETE CASCADE,
    target_store_path text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (derivation_id, target_store_path)
);

ALTER TABLE derivations
    ADD CONSTRAINT derivations_id_expected_store_path_unique
        UNIQUE (id, expected_store_path);

ALTER TABLE composite_policy_assessments
    ADD CONSTRAINT composite_policy_assessments_exact_target_fk
    FOREIGN KEY (derivation_id, target_store_path)
    REFERENCES composite_policy_derivation_targets (derivation_id, target_store_path)
    ON DELETE CASCADE;

ALTER TABLE composite_policy_assessments
    ADD CONSTRAINT composite_policy_assessments_fresh_expected_target_fk
    FOREIGN KEY (derivation_id, target_store_path)
    REFERENCES derivations (id, expected_store_path)
    ON DELETE CASCADE;

ALTER TABLE deployment_policy_versions
    ADD CONSTRAINT deployment_policy_versions_id_policy_id_unique UNIQUE (id, policy_id);

ALTER TABLE composite_policy_assessments
    ADD CONSTRAINT composite_policy_assessments_exact_version_fk
    FOREIGN KEY (policy_version_id, policy_lineage_id)
    REFERENCES deployment_policy_versions (id, policy_id) ON DELETE RESTRICT;

ALTER TABLE cve_scans
    ADD COLUMN composite_phase_order bigint;

-- Preserve the chronological order of scans that predate this migration. UUID
-- order is only a deterministic tie-breaker for equal creation timestamps.
WITH ordered AS (
    SELECT id, ROW_NUMBER() OVER (ORDER BY created_at NULLS FIRST, id) AS phase_order
    FROM cve_scans
)
UPDATE cve_scans scan
SET composite_phase_order = ordered.phase_order
FROM ordered
WHERE scan.id = ordered.id;

CREATE SEQUENCE cve_scans_composite_phase_order_seq;
ALTER SEQUENCE cve_scans_composite_phase_order_seq OWNED BY cve_scans.composite_phase_order;
SELECT setval(
    'cve_scans_composite_phase_order_seq',
    COALESCE((SELECT MAX(composite_phase_order) FROM cve_scans), 0) + 1,
    false
);
ALTER TABLE cve_scans
    ALTER COLUMN composite_phase_order
        SET DEFAULT nextval('cve_scans_composite_phase_order_seq'),
    ALTER COLUMN composite_phase_order SET NOT NULL,
    ADD CONSTRAINT cve_scans_composite_source_unique
        UNIQUE (id, composite_phase_order, derivation_id);

CREATE TABLE composite_policy_rule_results (
    assessment_id uuid NOT NULL REFERENCES composite_policy_assessments(id) ON DELETE CASCADE,
    rule_id uuid NOT NULL,
    ordinal integer NOT NULL,
    kind text NOT NULL,
    phase text NOT NULL,
    outcome text NOT NULL,
    blocking boolean NOT NULL,
    detail text NOT NULL,
    evidence jsonb NOT NULL DEFAULT '{}'::jsonb,
    source_scan_id uuid,
    source_scan_order bigint,
    source_scan_derivation_id integer,
    evaluated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (assessment_id, rule_id),
    UNIQUE (assessment_id, ordinal),
    CHECK (ordinal >= 0),
    CHECK (phase IN ('evaluation', 'scan', 'deployment')),
    CHECK (outcome IN ('pass', 'fail', 'error', 'not_checked')),
    CHECK (
        (phase = 'scan' AND source_scan_id IS NOT NULL AND source_scan_order IS NOT NULL
            AND source_scan_derivation_id IS NOT NULL)
        OR (phase <> 'scan' AND source_scan_id IS NULL AND source_scan_order IS NULL
            AND source_scan_derivation_id IS NULL)
        OR (phase = 'scan' AND outcome = 'not_checked' AND source_scan_id IS NULL
            AND source_scan_order IS NULL AND source_scan_derivation_id IS NULL)
    ),
    CONSTRAINT composite_policy_rule_results_scan_source_fk
        FOREIGN KEY (source_scan_id, source_scan_order, source_scan_derivation_id)
        REFERENCES cve_scans (id, composite_phase_order, derivation_id) ON DELETE RESTRICT
);

ALTER TABLE composite_policy_assessments
    ADD CONSTRAINT composite_policy_assessments_id_derivation_unique
        UNIQUE (id, derivation_id);

ALTER TABLE composite_policy_rule_results
    ADD CONSTRAINT composite_policy_rule_results_assessment_derivation_fk
        FOREIGN KEY (assessment_id, source_scan_derivation_id)
        REFERENCES composite_policy_assessments (id, derivation_id) ON DELETE CASCADE;

CREATE INDEX composite_policy_assessments_target_idx
    ON composite_policy_assessments (
        system_id, target_store_path, effective_set_digest, policy_version_id
    );

CREATE INDEX cve_scans_derivation_composite_order_idx
    ON cve_scans (derivation_id, composite_phase_order DESC);

-- Evaluation can terminate before Nix produces a derivation or store path.
-- Keep eval_passed evidence on the immutable attempt instead of inventing a
-- target identity that does not exist.
CREATE TABLE composite_eval_attempt_rule_results (
    evaluation_attempt_id uuid NOT NULL REFERENCES evaluation_attempts(id) ON DELETE CASCADE,
    system_id uuid REFERENCES systems(id) ON DELETE CASCADE,
    configuration_name text NOT NULL,
    policy_version_id uuid NOT NULL REFERENCES deployment_policy_versions(id) ON DELETE RESTRICT,
    rule_id uuid NOT NULL,
    kind text NOT NULL DEFAULT 'eval_passed' CHECK (kind = 'eval_passed'),
    outcome text NOT NULL CHECK (outcome IN ('pass', 'fail', 'error', 'not_checked')),
    detail text NOT NULL,
    evidence jsonb NOT NULL DEFAULT '{}'::jsonb,
    evaluated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    superseded_at timestamptz,
    PRIMARY KEY (evaluation_attempt_id, configuration_name, policy_version_id, rule_id)
);

CREATE INDEX composite_eval_attempt_rule_results_current_system_idx
    ON composite_eval_attempt_rule_results (system_id, policy_version_id, evaluated_at DESC)
    WHERE superseded_at IS NULL;

-- Snapshot only targets that predate the pending-delivery authorization
-- contract. Runtime authorization consumes these markers atomically; targets
-- issued after this migration must already have their ordinary pending row.
CREATE TABLE composite_legacy_desired_targets (
    system_id uuid PRIMARY KEY REFERENCES systems(id) ON DELETE CASCADE,
    target_store_path text NOT NULL,
    captured_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO composite_legacy_desired_targets (system_id, target_store_path)
SELECT id, desired_target
FROM systems
WHERE desired_target LIKE '/nix/store/%';
