-- 1) add an ID FK to what you actually deploy
ALTER TABLE systems
    ADD COLUMN IF NOT EXISTS desired_derivation_id int;

ALTER TABLE systems
    ADD CONSTRAINT fk_systems_desired_derivation FOREIGN KEY (desired_derivation_id) REFERENCES derivations (id);

-- 2) drop the incompatible text FK
ALTER TABLE systems
    DROP CONSTRAINT IF EXISTS fk_systems_desired_target;

