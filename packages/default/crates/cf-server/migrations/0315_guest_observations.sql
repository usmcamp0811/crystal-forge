-- Parent observations are append-only evidence. They never update system_states,
-- configuration evidence, deployment policy, Current CVEs or POA&M authority.
CREATE TABLE guest_observations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    guest_id uuid NOT NULL REFERENCES systems(id),
    parent_host_id uuid NOT NULL REFERENCES systems(id),
    environment_id uuid NOT NULL REFERENCES environments(id),
    guest_identity text NOT NULL,
    unit text NOT NULL,
    unit_active_state text,
    runner_store_path text,
    observed_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (guest_id <> parent_host_id),
    CHECK (observed_at <= received_at),
    CHECK (unit = 'microvm@' || guest_identity || '.service')
);
CREATE INDEX guest_observations_latest
    ON guest_observations (guest_id, observed_at DESC, id DESC);
COMMENT ON TABLE guest_observations IS
    'Append-only signed parent unit evidence, separate from guest runtime attestation and configuration/scan authority. Retained after mode and association changes.';
COMMENT ON COLUMN guest_observations.runner_store_path IS
    'Optional locally established runner store object, never a guest current-system closure or proof of a configuration match.';
