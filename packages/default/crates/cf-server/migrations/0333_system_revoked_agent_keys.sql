-- Records agent keys revoked when a registration leaves managed mode
-- (TASK-508 AC #6, #7).
--
-- Leaving managed sets systems.public_key to NULL. Without a record of that
-- key, a later conversion back into managed could re-enroll the same keypair,
-- so the agent identity that was revoked would be trusted again. The server
-- inserts the key here in the same transaction that revokes it, and refuses a
-- conversion or key rotation that presents a recorded key for the system.
--
-- Rows are history and are not updated. They are removed only with the system.
CREATE TABLE system_revoked_agent_keys (
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    public_key text NOT NULL CHECK (public_key !~ '^[[:space:]]*$'),
    revoked_at timestamptz NOT NULL DEFAULT NOW(),
    PRIMARY KEY (system_id, public_key)
);

COMMENT ON TABLE system_revoked_agent_keys IS
    'Agent keys revoked by a transition out of managed mode. A revoked key cannot re-enroll the same system.';
