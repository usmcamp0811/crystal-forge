-- 1) Add column
ALTER TABLE systems
    ADD COLUMN IF NOT EXISTS desired_derivation_id integer;

-- 2) Drop both old and new constraints (idempotent)
ALTER TABLE systems
    DROP CONSTRAINT IF EXISTS fk_systems_desired_target;

ALTER TABLE systems
    DROP CONSTRAINT IF EXISTS fk_systems_desired_derivation;

-- 3) Add new constraint (clean slate)
ALTER TABLE systems
    ADD CONSTRAINT fk_systems_desired_derivation FOREIGN KEY (desired_derivation_id) REFERENCES derivations (id);

