-- Explicit operator authority for display-only evaluation lifecycle metadata.
-- No backfill: absence and NULL both mean unknown, never established.
CREATE TABLE system_evaluation_lifecycle (
    system_id uuid PRIMARY KEY REFERENCES systems(id) ON DELETE CASCADE,
    greenfield boolean,
    classified_by uuid REFERENCES users(id) ON DELETE SET NULL,
    classified_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

COMMENT ON TABLE system_evaluation_lifecycle IS
    'Operator-managed display classification. Does not affect evaluation, counts, or deployment policy. Mutations and actor identity are recorded atomically in admin_audit_events.';
COMMENT ON COLUMN system_evaluation_lifecycle.greenfield IS
    'Explicit operator classification: true greenfield, false established, NULL unknown. Never inferred from system state or evaluation outcomes.';
COMMENT ON COLUMN system_evaluation_lifecycle.classified_by IS
    'Authenticated operator/admin actor. NULL only after user cleanup; durable audit actor_identifier retains the actor UUID.';
