-- Step 1: Create table without constraints
CREATE TABLE derivation_dependencies (
    derivation_id integer NOT NULL,
    depends_on_id integer NOT NULL,
    PRIMARY KEY (derivation_id, depends_on_id)
);

-- Step 2: Backfill
INSERT INTO derivation_dependencies (derivation_id, depends_on_id)
SELECT id, parent_derivation_id
FROM derivations
WHERE parent_derivation_id IS NOT NULL;

-- Step 3: Add constraints using DEFERRABLE INITIALLY DEFERRED
ALTER TABLE derivation_dependencies
    ADD CONSTRAINT fk_derivation_id FOREIGN KEY (derivation_id)
    REFERENCES derivations (id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE derivation_dependencies
    ADD CONSTRAINT fk_depends_on_id FOREIGN KEY (depends_on_id)
    REFERENCES derivations (id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED;

-- Step 4: Drop old FK/index/column
DROP INDEX IF EXISTS idx_derivations_parent_id;

ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS derivations_parent_derivation_id_fkey;

ALTER TABLE derivations
    DROP COLUMN IF EXISTS parent_derivation_id;
