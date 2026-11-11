-- Retain request identities after authority revocation. A later enrollment must
-- not revive a request created under an earlier agent's deployment authority.
ALTER TABLE deployment_request_reservations
    DROP CONSTRAINT deployment_request_reservations_state_check,
    ADD CONSTRAINT deployment_request_reservations_state_check
        CHECK (state IN ('reserved', 'conversion_persisted', 'deploy_failed',
                         'queued', 'invalidated'));
