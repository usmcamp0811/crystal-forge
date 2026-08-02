-- TASK-415: Deployment approval requests, decisions, and authorizations.
--
-- Replaces the simple deployment_approvals table (0122) with a richer request
-- lifecycle supporting multiple approvers, expiration, supersession, immutable
-- authorization records, and full audit trails.
--
-- This migration is additive: the old deployment_approvals table is preserved
-- (not dropped) since existing code references it. New code should use
-- deployment_approval_requests instead.

-- =============================================================================
-- Table: deployment_approval_requests
-- =============================================================================
-- A deployment approval request represents permission to deploy one exact
-- target to one exact system.

CREATE TABLE IF NOT EXISTS deployment_approval_requests (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id                   UUID NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    environment_id              UUID NULL REFERENCES environments(id) ON DELETE SET NULL,
    target_store_path           TEXT NOT NULL,
    target_derivation_path      TEXT NULL,
    target_commit_id            UUID NULL,
    target_commit_hash          TEXT NULL,
    flake_id                    UUID NULL REFERENCES flakes(id) ON DELETE SET NULL,
    deployment_policy_id        UUID NULL REFERENCES deployment_policies(id) ON DELETE SET NULL,
    deployment_policy_version_id UUID NULL,
    -- Requester
    requester_kind              TEXT NOT NULL,
    requested_by_user_id        UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    requested_by_automation     TEXT NULL,
    requested_at                TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Approval requirements (snapshot from policy at request time)
    required_approvals          INTEGER NOT NULL,
    required_role               TEXT NULL,
    distinct_approvers          BOOLEAN NOT NULL DEFAULT TRUE,
    requester_may_approve       BOOLEAN NOT NULL DEFAULT FALSE,
    expires_at                  TIMESTAMPTZ NULL,
    -- Status
    status                      TEXT NOT NULL DEFAULT 'pending',
    request_fingerprint         TEXT NOT NULL,
    -- Cross-references
    deployment_authorization_id UUID NULL,
    superseded_by_id            UUID NULL,
    decided_at                  TIMESTAMPTZ NULL,
    -- Timestamps
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT dar_required_approvals_positive CHECK (required_approvals > 0),
    CONSTRAINT dar_status_values CHECK (
        status IN ('pending', 'approved', 'rejected', 'expired', 'cancelled', 'superseded', 'consumed')
    ),
    CONSTRAINT dar_requester_kind_values CHECK (
        requester_kind IN ('user', 'automation')
    ),
    CONSTRAINT dar_user_requester CHECK (
        (requester_kind = 'user' AND requested_by_user_id IS NOT NULL)
        OR (requester_kind = 'automation' AND requested_by_automation IS NOT NULL)
    ),
    CONSTRAINT dar_no_self_supersede CHECK (superseded_by_id IS NULL OR superseded_by_id != id),
    CONSTRAINT dar_approved_has_authorization CHECK (
        status != 'approved' OR deployment_authorization_id IS NOT NULL
    )
);

-- Self-referencing FK for supersession chain
ALTER TABLE deployment_approval_requests
    ADD CONSTRAINT dar_superseded_by_fk
    FOREIGN KEY (superseded_by_id) REFERENCES deployment_approval_requests(id) ON DELETE SET NULL;

-- Partial unique: only one active request per fingerprint
CREATE UNIQUE INDEX IF NOT EXISTS idx_dar_active_fingerprint
    ON deployment_approval_requests (request_fingerprint)
    WHERE status = 'pending';

-- Query indexes
CREATE INDEX IF NOT EXISTS idx_dar_pending_by_system
    ON deployment_approval_requests (system_id, requested_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_dar_pending_by_environment
    ON deployment_approval_requests (environment_id, requested_at)
    WHERE status = 'pending' AND environment_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dar_pending_by_requested_at
    ON deployment_approval_requests (requested_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_dar_pending_by_expires
    ON deployment_approval_requests (expires_at)
    WHERE status = 'pending' AND expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dar_fingerprint
    ON deployment_approval_requests (request_fingerprint);

CREATE INDEX IF NOT EXISTS idx_dar_policy_version
    ON deployment_approval_requests (deployment_policy_version_id)
    WHERE deployment_policy_version_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dar_authorization
    ON deployment_approval_requests (deployment_authorization_id)
    WHERE deployment_authorization_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dar_status
    ON deployment_approval_requests (status);

COMMENT ON TABLE deployment_approval_requests IS
    'Deployment approval requests with full lifecycle, policy snapshots, and supersession tracking.';

COMMENT ON COLUMN deployment_approval_requests.request_fingerprint IS
    'Deterministic digest of immutable request inputs. Used for deduplication and supersession detection.';


-- =============================================================================
-- Table: deployment_approval_decisions
-- =============================================================================
-- Each decision (approve or reject) against a request.

CREATE TABLE IF NOT EXISTS deployment_approval_decisions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    request_id          UUID NOT NULL REFERENCES deployment_approval_requests(id) ON DELETE CASCADE,
    actor_user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    decision            TEXT NOT NULL,
    note                TEXT NULL,
    actor_role_snapshot TEXT NULL,
    request_fingerprint TEXT NOT NULL,
    status_before       TEXT NOT NULL,
    status_after        TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT dad_decision_values CHECK (decision IN ('approve', 'reject')),
    CONSTRAINT dad_unique_actor_per_request UNIQUE (request_id, actor_user_id)
);

CREATE INDEX IF NOT EXISTS idx_dad_request_created
    ON deployment_approval_decisions (request_id, created_at);

COMMENT ON TABLE deployment_approval_decisions IS
    'Immutable approval/rejection decisions on deployment approval requests.';


-- =============================================================================
-- Table: deployment_authorizations
-- =============================================================================
-- Immutable authorization records issued when an approval is completed or
-- when a policy does not require approval.

CREATE TABLE IF NOT EXISTS deployment_authorizations (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id                   UUID NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    target_store_path           TEXT NOT NULL,
    target_derivation_path      TEXT NULL,
    target_commit_id            UUID NULL,
    policy_version_id           UUID NULL,
    source_approval_request_id  UUID NULL REFERENCES deployment_approval_requests(id) ON DELETE SET NULL,
    authorization_source        TEXT NOT NULL,
    issued_by_user_id           UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    issued_by_automation        TEXT NULL,
    issued_at                   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_from                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at                  TIMESTAMPTZ NULL,
    revoked_at                  TIMESTAMPTZ NULL,
    consumed_at                 TIMESTAMPTZ NULL,
    deployment_execution_id     UUID NULL,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT da_source_values CHECK (
        authorization_source IN ('approval', 'policy_bypass', 'operator_adopt', 'automation')
    )
);

-- FK from deployment_approval_requests.deployment_authorization_id
ALTER TABLE deployment_approval_requests
    ADD CONSTRAINT dar_authorization_fk
    FOREIGN KEY (deployment_authorization_id) REFERENCES deployment_authorizations(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_da_system_target
    ON deployment_authorizations (system_id, target_store_path);

CREATE INDEX IF NOT EXISTS idx_da_active
    ON deployment_authorizations (system_id, valid_from)
    WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > NOW());

CREATE INDEX IF NOT EXISTS idx_da_approval_request
    ON deployment_authorizations (source_approval_request_id)
    WHERE source_approval_request_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_da_execution
    ON deployment_authorizations (deployment_execution_id)
    WHERE deployment_execution_id IS NOT NULL;

COMMENT ON TABLE deployment_authorizations IS
    'Immutable deployment authorization records. One authorization per approved or bypassed deployment.';


-- =============================================================================
-- Update attention_occurrences category constraint
-- =============================================================================
-- Add approval and attestation categories.

ALTER TABLE attention_occurrences DROP CONSTRAINT IF EXISTS attention_occurrences_category_check;
ALTER TABLE attention_occurrences ADD CONSTRAINT attention_occurrences_category_check CHECK (
    category IN ('builds', 'evals', 'flakes', 'systems', 'environments', 'cves', 'approvals', 'attestations')
);
