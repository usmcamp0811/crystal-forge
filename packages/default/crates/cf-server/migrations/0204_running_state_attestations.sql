-- TASK-415: Running-state attestations, trust classification, investigations,
-- and resolution actions.
--
-- Attestation rows are immutable after insertion. Classification and operator
-- actions are stored in separate tables.

-- =============================================================================
-- Table: running_state_attestations
-- =============================================================================
-- Immutable signed agent reports about the observed running system state.

CREATE TABLE IF NOT EXISTS running_state_attestations (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attestation_id              UUID NOT NULL,
    system_id                   UUID NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    agent_key_id                TEXT NOT NULL,
    agent_session_id            UUID NULL,
    protocol_version            INTEGER NOT NULL,
    boot_id                     TEXT NOT NULL,
    boot_timestamp              TIMESTAMPTZ NULL,
    observed_at                 TIMESTAMPTZ NOT NULL,
    received_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    monotonic_counter           BIGINT NOT NULL,
    current_system_store_path   TEXT NOT NULL,
    current_system_nar_hash     TEXT NULL,
    system_profile_store_path   TEXT NULL,
    booted_generation           BIGINT NULL,
    kernel_version              TEXT NULL,
    nix_version                 TEXT NULL,
    agent_version               TEXT NOT NULL,
    agent_build_hash            TEXT NULL,
    reported_authorization_id   UUID NULL,
    reported_execution_id       UUID NULL,
    activation_source           TEXT NULL,
    canonical_payload           BYTEA NOT NULL,
    payload_digest              BYTEA NOT NULL,
    signature                   BYTEA NOT NULL,
    verification_status         TEXT NOT NULL,
    verification_reason_code    TEXT NULL,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique attestation ID across all agents
    CONSTRAINT rsa_attestation_id_unique UNIQUE (attestation_id),

    -- Monotonic counter per agent key per boot session
    CONSTRAINT rsa_counter_unique UNIQUE (agent_key_id, boot_id, monotonic_counter),

    -- Valid verification statuses
    CONSTRAINT rsa_verification_status_values CHECK (
        verification_status IN (
            'verified',
            'invalid_signature',
            'unknown_key',
            'revoked_key',
            'identity_mismatch',
            'invalid_session',
            'replay',
            'stale_timestamp',
            'malformed'
        )
    ),

    -- Payload size guard (max 64 KiB canonical payload)
    CONSTRAINT rsa_payload_size CHECK (octet_length(canonical_payload) <= 65536),

    -- Signature size guard (max 256 bytes for Ed25519 + envelope)
    CONSTRAINT rsa_signature_size CHECK (octet_length(signature) <= 256),

    -- Payload digest size guard (max 64 bytes for SHA-512)
    CONSTRAINT rsa_digest_size CHECK (octet_length(payload_digest) <= 64)
);

-- Query indexes
CREATE INDEX IF NOT EXISTS idx_rsa_system_observed
    ON running_state_attestations (system_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS idx_rsa_system_received
    ON running_state_attestations (system_id, received_at DESC);

CREATE INDEX IF NOT EXISTS idx_rsa_verification_status
    ON running_state_attestations (verification_status);

CREATE INDEX IF NOT EXISTS idx_rsa_agent_key_boot
    ON running_state_attestations (agent_key_id, boot_id, monotonic_counter DESC);

COMMENT ON TABLE running_state_attestations IS
    'Immutable signed running-state attestations from enrolled agents. Signed fields are never updated after insertion.';


-- =============================================================================
-- Table: running_state_attestation_assessments
-- =============================================================================
-- Per-attestation trust classification results.

CREATE TABLE IF NOT EXISTS running_state_attestation_assessments (
    id                              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attestation_id                  UUID NOT NULL REFERENCES running_state_attestations(id) ON DELETE CASCADE,
    system_id                       UUID NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    classification                  TEXT NOT NULL,
    reason_code                     TEXT NOT NULL,
    matched_authorization_id        UUID NULL REFERENCES deployment_authorizations(id) ON DELETE SET NULL,
    matched_deployment_execution_id UUID NULL,
    matched_artifact_id             UUID NULL,
    classifier_version              INTEGER NOT NULL DEFAULT 1,
    assessed_at                     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One assessment per attestation
    CONSTRAINT rsaa_attestation_unique UNIQUE (attestation_id),

    -- Valid classifications
    CONSTRAINT rsaa_classification_values CHECK (
        classification IN (
            'authorized_current',
            'authorized_but_evidence_stale',
            'authorized_previous_generation',
            'deployment_pending_reboot',
            'activation_failed',
            'unauthorized_artifact',
            'unknown_artifact',
            'agent_attestation_stale',
            'agent_identity_invalid'
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_rsaa_system_classification
    ON running_state_attestation_assessments (system_id, classification);

CREATE INDEX IF NOT EXISTS idx_rsaa_classification_assessed
    ON running_state_attestation_assessments (classification, assessed_at);

COMMENT ON TABLE running_state_attestation_assessments IS
    'Immutable per-attestation trust classification. Each attestation gets exactly one assessment.';


-- =============================================================================
-- Table: system_trust_states
-- =============================================================================
-- Current projected trust state for each system. Updated on every
-- attestation arrival and by periodic reconciliation.

CREATE TABLE IF NOT EXISTS system_trust_states (
    system_id                   UUID PRIMARY KEY REFERENCES systems(id) ON DELETE CASCADE,
    current_classification      TEXT NOT NULL,
    reason_code                 TEXT NOT NULL,
    latest_attestation_id       UUID NULL REFERENCES running_state_attestations(id) ON DELETE SET NULL,
    latest_authorization_id     UUID NULL REFERENCES deployment_authorizations(id) ON DELETE SET NULL,
    observed_store_path         TEXT NULL,
    expected_store_path         TEXT NULL,
    evidence_age_seconds        BIGINT NULL,
    investigation_id            UUID NULL,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sts_classification_values CHECK (
        current_classification IN (
            'authorized_current',
            'authorized_but_evidence_stale',
            'authorized_previous_generation',
            'deployment_pending_reboot',
            'activation_failed',
            'unauthorized_artifact',
            'unknown_artifact',
            'agent_attestation_stale',
            'agent_identity_invalid',
            'no_attestation'
        )
    )
);

COMMENT ON TABLE system_trust_states IS
    'Current projected trust state per system. Changes when a new attestation arrives, when time-based staleness thresholds are crossed, or when operator resolution actions occur.';


-- =============================================================================
-- Table: attestation_investigations
-- =============================================================================

CREATE TABLE IF NOT EXISTS attestation_investigations (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id           UUID NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    source_attestation_id UUID NOT NULL REFERENCES running_state_attestations(id) ON DELETE CASCADE,
    status              TEXT NOT NULL DEFAULT 'open',
    opened_by_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    owner_user_id       UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    opening_note        TEXT NOT NULL,
    opened_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_by_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    resolution_reason   TEXT NULL,
    resolution_note     TEXT NULL,
    resolved_at         TIMESTAMPTZ NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT ai_status_values CHECK (status IN ('open', 'resolved')),
    CONSTRAINT ai_resolved_fields CHECK (
        (status = 'open' AND resolved_at IS NULL)
        OR (status = 'resolved' AND resolved_at IS NOT NULL AND resolved_by_user_id IS NOT NULL)
    )
);

-- At most one open investigation per system
CREATE UNIQUE INDEX IF NOT EXISTS idx_ai_open_per_system
    ON attestation_investigations (system_id)
    WHERE status = 'open';

-- FK from system_trust_states
ALTER TABLE system_trust_states
    ADD CONSTRAINT sts_investigation_fk
    FOREIGN KEY (investigation_id) REFERENCES attestation_investigations(id) ON DELETE SET NULL;

COMMENT ON TABLE attestation_investigations IS
    'Investigation cases opened by operators for suspicious running-state trust conditions. At most one open case per system.';


-- =============================================================================
-- Table: attestation_resolution_actions
-- =============================================================================

CREATE TABLE IF NOT EXISTS attestation_resolution_actions (
    id                              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id                       UUID NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    attestation_id                  UUID NOT NULL REFERENCES running_state_attestations(id) ON DELETE CASCADE,
    actor_user_id                   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    action                          TEXT NOT NULL,
    note                            TEXT NOT NULL,
    created_authorization_id        UUID NULL REFERENCES deployment_authorizations(id) ON DELETE SET NULL,
    created_deployment_request_id   UUID NULL REFERENCES deployment_approval_requests(id) ON DELETE SET NULL,
    investigation_id                UUID NULL REFERENCES attestation_investigations(id) ON DELETE SET NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT ara_action_values CHECK (
        action IN ('adopt', 'replace', 'investigate', 'close_investigation')
    )
);

CREATE INDEX IF NOT EXISTS idx_ara_system
    ON attestation_resolution_actions (system_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ara_attestation
    ON attestation_resolution_actions (attestation_id);

COMMENT ON TABLE attestation_resolution_actions IS
    'Immutable record of operator resolution actions on running-state trust conditions.';
