-- Add build progress tracking columns to derivations table
ALTER TABLE derivations
    ADD COLUMN build_elapsed_seconds integer;

ALTER TABLE derivations
    ADD COLUMN build_current_target text;

ALTER TABLE derivations
    ADD COLUMN build_last_activity_seconds integer;

ALTER TABLE derivations
    ADD COLUMN build_last_heartbeat timestamp with time zone;

