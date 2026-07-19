DROP VIEW IF EXISTS view_systems_status_table CASCADE;

UPDATE
    derivation_statuses
SET
    is_terminal = TRUE
WHERE
    name = 'build-complete';

