-- TASK-440: permit direct immutable-source Config observation requests.
--
-- Migration 0266 enabled schema version 2 content and observations. Requests
-- carry the same schema identity and must accept version 2 before queueing or
-- publishing a direct shallow observation.

ALTER TABLE config_observation_requests
    DROP CONSTRAINT config_observation_requests_schema_version_check,
    ADD CONSTRAINT config_observation_requests_schema_version_check
        CHECK (schema_version IN (1, 2));
