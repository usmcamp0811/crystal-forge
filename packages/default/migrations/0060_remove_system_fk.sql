-- Add new column
ALTER TABLE systems
    ADD COLUMN IF NOT EXISTS desired_derivation_id integer;

-- Drop old constraint before adding new one
ALTER TABLE systems
    DROP CONSTRAINT IF EXISTS fk_systems_desired_target;

-- Add new foreign key constraint
ALTER TABLE systems
    ADD CONSTRAINT fk_systems_desired_derivation FOREIGN KEY (desired_derivation_id) REFERENCES derivations (id);

