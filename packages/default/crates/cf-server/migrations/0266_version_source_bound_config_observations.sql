-- TASK-440: distinguish direct immutable-source Config observations.
--
-- Schema version 1 observations remain readable. Version 2 identifies results
-- produced by the bounded shallow extractor against the server-published,
-- NAR-qualified immutable source. This evidence remains observational only.

ALTER TABLE config_observation_contents
    DROP CONSTRAINT config_observation_contents_schema_version_check,
    ADD CONSTRAINT config_observation_contents_schema_version_check
        CHECK (schema_version IN (1, 2));

ALTER TABLE config_observations
    DROP CONSTRAINT config_observations_schema_version_check,
    ADD CONSTRAINT config_observations_schema_version_check
        CHECK (schema_version IN (1, 2));
